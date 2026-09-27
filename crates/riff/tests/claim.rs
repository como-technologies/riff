//! `riff claim`, `riff release`, `riff post`, `riff tell`, `riff read`,
//! `riff lead` and `riff status` from the command line, against a real
//! server.

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
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("XDG_RUNTIME_DIR", dir);
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

/// Resumes the new riff as the person mike, in a shell.
async fn resume(server: &str, dir: &Path) {
    let (out, code) = run(server, dir, "mike", None, &["resume"]).await;
    assert_eq!(code, 0, "{out}");
}

#[tokio::test]
async fn a_new_riff_is_paused_and_refuses_each_claim() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    let (out, code) = run(&server, dir, "mike", None, &["who"]).await;
    assert_eq!(code, 0);
    assert!(out.starts_with("The riff is paused."), "{out}");
    let (_, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_ne!(code, 0, "a paused riff refuses a claim");

    let (out, code) = run(&server, dir, "mike", None, &["resume"]).await;
    assert_eq!(code, 0);
    assert_eq!(out, "The riff is running now. No other session woke.\n");
    let (out, _) = run(&server, dir, "mike", None, &["resume"]).await;
    assert_eq!(out, "The riff was running already.\n");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.ends_with("The riff is running.\n"), "{out}");
    let (_, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_eq!(code, 0);

    // The first session of brett is its lead. A second session of brett
    // is not, so it cannot pause the riff.
    agent(&server, dir, "brett", "first", &["who"]).await;
    let (_, code) = agent(&server, dir, "brett", "second", &["pause"]).await;
    assert_ne!(code, 0, "only a person or a lead can pause");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.ends_with("The riff is running.\n"), "{out}");

    let (out, code) = run(&server, dir, "mike", None, &["pause"]).await;
    assert_eq!(code, 0);
    assert!(out.starts_with("The riff is paused now."), "{out}");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.contains("The riff is paused."), "{out}");
}

#[tokio::test]
async fn claim_and_release_work_in_the_repository_thread() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    resume(&server, dir).await;

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
    resume(&server, dir.path()).await;

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
    resume(&server, dir).await;

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
        format!(
            "{note}\n\ncomo-technologies/riff\n[1] riff://mike@pangolin (verified): the API is ready\n"
        )
    );
    assert_eq!(code, 0);

    let (out, _) = riff(&server, dir, "brett", &["read"]).await;
    assert_eq!(out, "No unread messages.\n");

    let (out, _) = riff(&server, dir, "brett", &["read", "--all"]).await;
    assert!(
        out.contains("[1] riff://mike@pangolin (verified): the API is ready"),
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
        out.ends_with("api-v2\n[1] riff://mike@pangolin (verified): v2 plan\n"),
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
             [1] riff://mike@pangolin to session=b2 (verified): are you there?\n\
             [2] riff://mike@pangolin to session=b2 (verified): and now?\n"
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

#[tokio::test]
async fn lead_marks_the_lead_and_tell_lead_reaches_it() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    agent(&server, dir, "mike", "a1", &["read"]).await;
    agent(&server, dir, "mike", "b2", &["read"]).await;
    let (out, _) = riff(&server, dir, "mike", &["who"]).await;
    assert!(out.contains("?session=a1&lead=true\n"), "{out}");
    assert!(out.contains("?session=b2\n"), "{out}");

    let (out, code) = agent(&server, dir, "mike", "b2", &["lead"]).await;
    assert_eq!(
        out,
        "You are the lead of mike in como-technologies/riff. \
         mike@pangolin:riff (a1) is not the lead now.\n"
    );
    assert_eq!(code, 0);

    let (out, code) = agent(&server, dir, "mike", "a1", &["tell", "lead", "merge?"]).await;
    assert_eq!(
        out,
        "Posted message 1 to a direct thread. Woke mike@pangolin:riff (b2).\n"
    );
    assert_eq!(code, 0);

    let (_, code) = riff(&server, dir, "mike", &["lead"]).await;
    assert_ne!(code, 0, "a person with no session ID cannot be the lead");
}

#[tokio::test]
async fn a_person_asks_for_status_and_who_shows_each_answer() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    agent(&server, dir, "mike", "a1", &["read"]).await;
    agent(&server, dir, "mike", "b2", &["read"]).await;
    let ask = [
        "post",
        "--kind",
        "status",
        "--to",
        "repo=como-technologies/riff",
    ];
    let (out, code) = riff(&server, dir, "mike", &ask).await;
    assert_eq!(
        out,
        "Posted message 1 to como-technologies/riff. \
         Woke mike@pangolin:riff (a1), mike@pangolin:riff (b2).\n"
    );
    assert_eq!(code, 0);

    let (out, _) = agent(&server, dir, "mike", "a1", &["read"]).await;
    assert!(
        out.ends_with(
            "[1] riff://mike@pangolin to repo=como-technologies/riff (verified) asks for your status.\n"
        ),
        "{out}"
    );
    let (out, code) = agent(
        &server,
        dir,
        "mike",
        "a1",
        &["status", "write", "the", "tests"],
    )
    .await;
    assert_eq!(out, "Your status is now: write the tests\n");
    assert_eq!(code, 0);
    let blocked = ["status", "--blocked", "waits for a review", "merge"];
    let (_, code) = agent(&server, dir, "mike", "b2", &blocked).await;
    assert_eq!(code, 0);

    let (out, _) = riff(&server, dir, "mike", &["who"]).await;
    assert!(
        out.contains("?session=a1&lead=true\n  status 0s ago: write the tests\n"),
        "{out}"
    );
    assert!(
        out.contains("?session=b2\n  blocked 0s ago: waits for a review (step: merge)\n"),
        "{out}"
    );

    let (_, code) = riff(&server, dir, "mike", &["post"]).await;
    assert_ne!(code, 0, "a message needs a body");
}
