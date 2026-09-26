//! `riff claim`, `riff release`, `riff post`, `riff tell` and `riff read`
//! from the command line, against a real server.

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

/// Runs `riff` as one person in `dir`, outside any agent session. Returns stdout and the exit code.
async fn riff(server: &str, dir: &Path, user: &str, args: &[&str]) -> (String, i32) {
    run(server, dir, user, None, args).await
}

/// Runs `riff` in `dir` as the agent session `session` of `user`.
async fn agent(
    server: &str,
    dir: &Path,
    user: &str,
    session: &str,
    args: &[&str],
) -> (String, i32) {
    run(server, dir, user, Some(session), args).await
}

async fn run(
    server: &str,
    dir: &Path,
    user: &str,
    session: Option<&str>,
    args: &[&str],
) -> (String, i32) {
    let mut cmd = Command::cargo_bin("riff").unwrap();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", user)
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
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
        "mike@pangolin holds issue-12 in como-technologies/riff.\n"
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

#[tokio::test]
async fn post_wakes_the_holder_of_a_claim() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    riff(&server, dir, "mike", &["claim", "issue-6"]).await;
    let (out, code) = riff(
        &server,
        dir,
        "brett",
        &[
            "post",
            "--to",
            "claim=issue-6",
            "--to",
            "user=ghost",
            "status?",
        ],
    )
    .await;
    assert_eq!(
        out,
        "Posted message 1 to como-technologies/riff. Woke mike@pangolin. \
         No session matches user=ghost.\n"
    );
    assert_eq!(code, 0);
}

#[tokio::test]
async fn watch_needs_a_session_id() {
    let server = start_server().await;
    let dir = repo();
    let mut cmd = Command::cargo_bin("riff").unwrap();
    cmd.arg("watch")
        .current_dir(dir.path())
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no session ID"));
}

#[tokio::test]
async fn read_shows_unread_messages_of_the_repository_thread() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let note = riff::text::DATA_NOTE;

    let (out, code) = riff(&server, dir, "brett", &["read"]).await;
    assert_eq!((out.as_str(), code), ("No unread messages.\n", 0));

    riff(&server, dir, "mike", &["post", "the API is ready"]).await;
    let (out, code) = riff(&server, dir, "brett", &["read"]).await;
    assert_eq!(
        out,
        format!("{note}\n\ncomo-technologies/riff\n[1] riff://mike@pangolin: the API is ready\n")
    );
    assert_eq!(code, 0);

    let (out, _) = riff(&server, dir, "brett", &["read"]).await;
    assert_eq!(out, "No unread messages.\n");

    let (out, _) = riff(&server, dir, "brett", &["read", "--all"]).await;
    assert!(
        out.contains("[1] riff://mike@pangolin: the API is ready"),
        "{out}"
    );
}

#[tokio::test]
async fn read_takes_a_named_thread() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    riff(
        &server,
        dir,
        "mike",
        &["post", "--thread", "api-v2", "v2 plan"],
    )
    .await;
    let (out, code) = riff(&server, dir, "brett", &["read", "--thread", "api-v2"]).await;
    assert!(
        out.ends_with("api-v2\n[1] riff://mike@pangolin: v2 plan\n"),
        "{out}"
    );
    assert_eq!(code, 0);

    let (_, code) = riff(&server, dir, "brett", &["read", "--thread", "nothing"]).await;
    assert_ne!(code, 0, "an unknown thread must fail the command");
}

#[tokio::test]
async fn tell_sends_a_direct_message_to_a_session() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let uri = "riff://brett@pangolin/como-technologies/riff?session=b2";

    agent(&server, dir, "brett", "b2", &["read"]).await;
    let (out, code) = riff(
        &server,
        dir,
        "mike",
        &["tell", "b2", "are", "you", "there?"],
    )
    .await;
    assert_eq!(
        out,
        "Posted message 1 to a direct thread. Woke brett@pangolin:riff (b2).\n"
    );
    assert_eq!(code, 0);

    let (out, code) = riff(&server, dir, "mike", &["tell", uri, "and now?"]).await;
    assert_eq!(
        out,
        "Posted message 2 to a direct thread. Woke brett@pangolin:riff (b2).\n"
    );
    assert_eq!(code, 0);

    let (out, _) = agent(&server, dir, "brett", "b2", &["read"]).await;
    assert!(
        out.ends_with(
            "direct with mike@pangolin\n\
             [1] riff://mike@pangolin to session=b2: are you there?\n\
             [2] riff://mike@pangolin to session=b2: and now?\n"
        ),
        "{out}"
    );

    let (_, code) = riff(
        &server,
        dir,
        "mike",
        &["tell", "riff://brett@pangolin", "hi"],
    )
    .await;
    assert_ne!(code, 0, "a URI with no session ID must fail the command");
}
