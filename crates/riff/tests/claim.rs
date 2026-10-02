//! `riff claim`, `riff release`, `riff post`, `riff tell`, `riff read`,
//! `riff lead` and `riff status` from the command line, against a real
//! server.

use isolated::Isolated;
use std::path::Path;
use std::process::Command as Git;

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
    repo_named("riff")
}

/// A git repository with the GitHub origin `como-technologies/NAME`.
fn repo_named(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    let origin = format!("https://github.com/como-technologies/{name}.git");
    git(dir.path(), &["remote", "add", "origin", &origin]);
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
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", user)
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir);
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
    let (out, code) = run(server, dir, "mike", None, &["resume", "--riff"]).await;
    assert_eq!(code, 0, "{out}");
}

#[tokio::test]
async fn a_new_riff_is_paused_and_refuses_each_claim() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    let (out, code) = run(&server, dir, "mike", None, &["who"]).await;
    assert_eq!(code, 0);
    assert!(out.starts_with("riff   paused by the server\n"), "{out}");
    let (_, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_ne!(code, 0, "a paused riff refuses a claim");

    // A resume of the repository leaves the new riff paused.
    let (out, code) = run(&server, dir, "mike", None, &["resume"]).await;
    assert_eq!(code, 0);
    assert_eq!(
        out,
        "The repository como-technologies/riff was running already. The whole riff is still \
         paused: the owner or an admin resumes it with `riff resume --riff`.\n"
    );
    let (_, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_ne!(code, 0, "the riff is still paused");

    let (out, code) = run(&server, dir, "mike", None, &["resume", "--riff"]).await;
    assert_eq!(code, 0);
    assert_eq!(out, "The riff is running now. No other session woke.\n");
    let (out, _) = run(&server, dir, "mike", None, &["resume", "--riff"]).await;
    assert_eq!(out, "The riff was running already.\n");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.contains("\nriff     running\n"), "{out}");
    let (_, code) = riff(&server, dir, "mike", &["claim", "issue-12"]).await;
    assert_eq!(code, 0);

    // The first session of brett is its lead. A second session of brett
    // is not, so it cannot pause the riff.
    agent(&server, dir, "brett", "first", &["who"]).await;
    let (_, code) = agent(&server, dir, "brett", "second", &["pause"]).await;
    assert_ne!(code, 0, "only a person or a lead can pause");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.contains("\nriff     running\n"), "{out}");

    let (out, code) = run(&server, dir, "mike", None, &["pause", "--riff"]).await;
    assert_eq!(code, 0);
    assert!(out.starts_with("The riff is paused now."), "{out}");
    let (out, _) = run(&server, dir, "mike", None, &["whoami"]).await;
    assert!(out.contains("\nriff     paused by the person mike\n"), "{out}");
    assert!(out.contains("The riff is paused."), "{out}");
}

/// Two repositories. The lead of strata runs `riff pause`: only strata
/// stops, and `whoami` and `who` show which pause it is and who set it
/// (01M3XAHZBGSSJB3YX23K88W01K, 01M3XAHZJAF6YVDJ7WX74X8RBX). `--repo`
/// names a repository from another directory.
#[tokio::test]
async fn pause_and_resume_name_the_repository_of_the_directory() {
    let server = start_server().await;
    let (here, strata) = (repo(), repo_named("strata"));
    let (here, strata) = (here.path(), strata.path());
    resume(&server, here).await;
    agent(&server, here, "mike", "a1", &["who"]).await;
    agent(&server, strata, "brett", "b1", &["who"]).await;

    let (out, code) = agent(&server, strata, "brett", "b1", &["pause"]).await;
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "The repository como-technologies/strata is paused now. No other session woke.\n"
    );

    // The session in strata is paused, and its claim is refused.
    let (out, _) = agent(&server, strata, "brett", "b1", &["whoami"]).await;
    assert!(out.contains("\nriff     running\n"), "{out}");
    let fact = "\npaused   como-technologies/strata by the session brett/b1\n";
    assert!(out.contains(fact), "{out}");
    assert!(
        out.contains(
            "The repository como-technologies/strata is paused. Nobody claims work there. Your \
             user or the lead resumes it with: riff resume"
        ),
        "{out}"
    );
    let (_, code) = agent(&server, strata, "brett", "b1", &["claim", "issue-7"]).await;
    assert_ne!(code, 0, "a paused repository refuses a claim");

    // The other repository goes on. It sees the pause, with no action.
    let (out, code) = agent(&server, here, "mike", "a1", &["claim", "issue-12"]).await;
    assert_eq!(code, 0, "{out}");
    let (out, _) = agent(&server, here, "mike", "a1", &["whoami"]).await;
    assert!(out.contains("\nriff     running\n"), "{out}");
    assert!(out.contains(fact), "{out}");
    assert!(!out.contains("Nobody claims work"), "{out}");
    let (out, _) = agent(&server, here, "mike", "a1", &["who"]).await;
    assert!(out.starts_with("riff    running\n"), "{out}");
    assert!(
        out.contains("\npaused  como-technologies/strata by the session brett/b1\n"),
        "{out}"
    );

    // A person resumes strata by its name, from another directory.
    let resume = ["resume", "--repo", "como-technologies/strata"];
    let (out, code) = run(&server, here, "mike", None, &resume).await;
    assert_eq!(code, 0, "{out}");
    assert!(
        out.starts_with("The repository como-technologies/strata is running now."),
        "{out}"
    );
    let (out, code) = agent(&server, strata, "brett", "b1", &["claim", "issue-7"]).await;
    assert_eq!(code, 0, "{out}");
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
    let mut cmd = Isolated::shared().assert_riff();
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
            "{note}\n\ncomo-technologies/riff\n[1] mike@pangolin (verified): the API is ready\n"
        )
    );
    assert_eq!(code, 0);

    let (out, _) = riff(&server, dir, "brett", &["read"]).await;
    assert_eq!(out, "No unread messages.\n");

    let (out, _) = riff(&server, dir, "brett", &["read", "--all"]).await;
    assert!(
        out.contains("[1] mike@pangolin (verified): the API is ready"),
        "{out}"
    );
}

/// The server gives one page of messages for each read. `riff read`
/// and `riff read --all` read each page (01M3TBZBX140GJWCV5GZ73Q5Z5).
#[tokio::test]
async fn read_all_reads_each_page() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let count = riff_server::state::PAGE + 10;
    for n in 1..=count {
        riff(&server, dir, "mike", &["post", &format!("message {n}")]).await;
    }
    for args in [&["read"][..], &["read", "--all"]] {
        let (out, code) = riff(&server, dir, "brett", args).await;
        assert_eq!(code, 0);
        assert_eq!(
            out.matches("mike@pangolin (verified)").count(),
            count,
            "{out}"
        );
        assert!(out.contains(&format!(
            "[{count}] mike@pangolin (verified): message {count}"
        )));
        assert!(!out.contains("More messages follow"), "{out}");
    }
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
        out.ends_with("api-v2\n[1] mike@pangolin (verified): v2 plan\n"),
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
             [1] mike@pangolin to session=b2 (verified): are you there?\n\
             [2] mike@pangolin to session=b2 (verified): and now?\n"
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

    agent(&server, dir, "mike", "a1", &["lead"]).await;
    agent(&server, dir, "mike", "b2", &["read"]).await;
    let (out, _) = riff(&server, dir, "mike", &["who", "--long"]).await;
    assert!(out.contains("?session=a1&lead=true  "), "{out}");
    assert!(out.contains("?session=b2  "), "{out}");

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

/// The lead frees the claim of another session of its user, and the
/// next session claims the item. A note of the server in the thread
/// names the lead, the item and the holder. A session that is not the
/// lead is refused, and so is the lead of another user
/// (01M3WG243BW7P6E1ME0DFNQF8C).
#[tokio::test]
async fn the_lead_releases_the_claim_of_another_session() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    agent(&server, dir, "mike", "lead1", &["lead"]).await;
    agent(&server, dir, "brett", "b1", &["lead"]).await;
    resume(&server, dir).await;
    let (out, code) = agent(&server, dir, "mike", "work1", &["claim", "issue-12"]).await;
    assert_eq!(code, 0, "{out}");

    let free = ["release", "issue-12", "--session", "work1"];
    let (out, code) = agent(&server, dir, "mike", "work2", &free).await;
    assert_ne!(code, 0, "a session that is not the lead: {out}");
    let (out, code) = agent(&server, dir, "brett", "b1", &free).await;
    assert_ne!(code, 0, "the lead of another user: {out}");
    let (out, code) = riff(&server, dir, "mike", &free).await;
    assert_ne!(code, 0, "a person is not the lead: {out}");
    let other = ["release", "issue-12", "--session", "work2"];
    let (out, code) = agent(&server, dir, "mike", "lead1", &other).await;
    assert_ne!(code, 0, "work2 does not hold the item: {out}");
    let (_, code) = agent(&server, dir, "mike", "work2", &["claim", "issue-12"]).await;
    assert_eq!(code, 1, "work1 still holds the item");

    // The start of the session ID names the holder, as `riff who` shows it.
    let (out, code) = agent(
        &server,
        dir,
        "mike",
        "lead1",
        &["release", "issue-12", "--session", "work"],
    )
    .await;
    assert_eq!(
        out,
        "You released issue-12 in como-technologies/riff for the session work. \
         The item is free.\n"
    );
    assert_eq!(code, 0);
    let (out, code) = agent(&server, dir, "mike", "work2", &["claim", "issue-12"]).await;
    assert_eq!(code, 0, "{out}");

    let (out, _) = agent(&server, dir, "mike", "work1", &["read"]).await;
    assert!(
        out.contains(
            "claims: the lead mike@pangolin:riff (lead1) released issue-12 for the session \
             mike@pangolin:riff (work1). issue-12 is free."
        ),
        "{out}"
    );
}

#[tokio::test]
async fn a_person_asks_for_status_and_who_shows_each_answer() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    agent(&server, dir, "mike", "a1", &["lead"]).await;
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
        out.ends_with("[1] mike@pangolin to all (verified) asks for your status.\n"),
        "{out}"
    );
    // A running riff, so that paused does not hide the steps.
    let (_, code) = agent(&server, dir, "mike", "a1", &["resume", "--riff"]).await;
    assert_eq!(code, 0);
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

    // With a watch, each session is live, so who shows its state and
    // its step (01M3QB6CJ1XCQG5B1BVR8AF3B4).
    let api = riff::api::Api::new(&server);
    let uri = |id: &str| -> riff_core::name::SessionUri {
        format!("riff://mike@pangolin/como-technologies/riff?session={id}")
            .parse()
            .unwrap()
    };
    let (a1, b2) = (uri("a1"), uri("b2"));
    let _a1 = api.watch(&a1).await.unwrap();
    let _b2 = api.watch(&b2).await.unwrap();
    let (out, _) = riff(&server, dir, "mike", &["who"]).await;
    // The age of a step is the time since the session set it. The test
    // checks that the age is there, not its number: a machine under
    // load takes more than a second.
    let row = |id: &str| -> String {
        let row = out.lines().find(|l| l.contains(id)).unwrap();
        let Some(end) = row.rfind("s ago") else {
            return row.to_owned();
        };
        let digits = row[..end].chars().rev().take_while(char::is_ascii_digit);
        let start = end - digits.count();
        assert!(start < end, "an age in seconds: {row}");
        format!("{}AGE{}", &row[..start], &row[end + 1..])
    };
    assert!(row("(a1)").ends_with("  AGE ago: write the tests"), "{out}");
    assert!(row("(b2)").contains("  blocked  "), "{out}");
    assert!(
        row("(b2)").ends_with("  waits for a review (step: merge, AGE ago)"),
        "{out}"
    );

    let (_, code) = riff(&server, dir, "mike", &["post"]).await;
    assert_ne!(code, 0, "a message needs a body");
}
