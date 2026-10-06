//! The look of each riff command for people (01M3Q5V313XQN86BA2PBTXHEZC),
//! with `--color never` (01M3Q5VE2D244XDZRYXM8DNSRS), and usage errors
//! that do not name `--server` (01M3Q5VE4VVXT9FH4J4MAWX68V).

use isolated::Isolated;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const THREAD: &str = "como-technologies/riff";

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

/// `riff ARGS --color never` in `dir`, as the agent session `session` of
/// mike, or as the person mike. Each test has its own environment, so
/// its settings are its own.
fn riff(env: &Isolated, server: &str, dir: &Path, session: Option<&str>, args: &[&str]) -> Command {
    let mut cmd = env.riff();
    cmd.args(args)
        .args(["--color", "never"])
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_USER", "mike")
        .env("CLICOLOR_FORCE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(id) = session {
        cmd.env("RIFF_SESSION", id);
    }
    cmd
}

async fn output(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

/// The stdout of `cmd`. It exits with 0 and has no escape code.
async fn stdout(cmd: Command) -> String {
    let out = output(cmd).await;
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!text.contains('\x1b'), "{text:?}");
    text
}

#[tokio::test(flavor = "multi_thread")]
async fn a_usage_error_does_not_name_server() {
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    for args in [&["owner"][..], &["invite"], &["tell"]] {
        // With no --color: an option on the command line shows in the
        // usage.
        let mut cmd = env.riff();
        cmd.args(args)
            .current_dir(dir.path())
            .env("RIFF_SERVER", "https://riff.example.com");
        let out = output(cmd).await;
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(err.contains("Usage: riff "), "{err}");
        assert!(!err.contains("--server"), "{err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn riff_server_is_the_server_of_riff() {
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    let out = stdout(riff(
        &env,
        "riff.example.com",
        dir.path(),
        None,
        &["server"],
    ))
    .await;
    assert!(
        out.contains("\nserver      http://riff.example.com:7878  (from RIFF_SERVER)\n"),
        "{out}"
    );
    let mut flag = riff(&env, "riff.example.com", dir.path(), None, &["server"]);
    flag.args(["--server", "other.example.com"]);
    let out = stdout(flag).await;
    assert!(
        out.contains("http://other.example.com:7878  (from --server)"),
        "{out}"
    );
    let bad = output(riff(&env, "a:b:c:d", dir.path(), None, &["server"])).await;
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("RIFF_SERVER"));
}

#[tokio::test(flavor = "multi_thread")]
async fn whoami_shows_facts() {
    let env = Isolated::new();
    let server = start_server().await;
    let dir = repo();
    let out = stdout(riff(
        &env,
        &server,
        dir.path(),
        Some("a6cf2205-1"),
        &["whoami"],
    ))
    .await;
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "session  mike@pangolin:riff (a6cf2205)", "{out}");
    assert_eq!(
        lines[1],
        format!("uri      riff://mike@pangolin/{THREAD}?session=a6cf2205-1"),
        "{out}"
    );
    assert_eq!(lines[2], "riff     paused by the server", "{out}");
    assert!(lines[3].starts_with("build    v"), "{out}");
    // The action comes last.
    assert!(
        out.ends_with("resumes it with: riff resume --riff\n"),
        "{out}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn who_is_a_table_and_long_shows_the_uri() {
    let env = Isolated::new();
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let status = ["status", "write the tests"];
    stdout(riff(&env, &server, dir, Some("a6cf2205-1"), &["lead"])).await;
    stdout(riff(&env, &server, dir, Some("a6cf2205-1"), &status)).await;
    let out = stdout(riff(&env, &server, dir, Some("a6cf2205-1"), &["who"])).await;
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "riff   paused by the server", "{out}");
    assert_eq!(lines[2], "", "{out}");
    assert_eq!(
        lines[3], "SESSION                        STATE   ROLE      DETAIL",
        "{out}"
    );
    assert_eq!(
        lines[4],
        "mike@pangolin:riff (a6cf2205)  paused  you lead  stopped at: write the tests",
        "{out}"
    );
    assert!(!out.contains("riff://"), "{out}");

    let mut long = riff(&env, &server, dir, Some("a6cf2205-1"), &["who"]);
    long.arg("--long");
    let out = stdout(long).await;
    let uri = format!("riff://mike@pangolin/{THREAD}?session=a6cf2205-1&lead=true");
    assert!(out.contains("\nURI  "), "{out}");
    assert!(
        out.contains(&format!("\n{uri}  paused  you lead  stopped at: ")),
        "{out}"
    );
    assert!(!out.contains("(a6cf2205)"), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_setting_shows_its_file_and_a_hint_last() {
    let env = Isolated::new();
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let file = env.riff_home().join("config.toml");
    let file = file.display();
    let cases: [(&[&str], String, &str); 4] = [
        (
            &["workers", "limit", "2"],
            format!("workers.limit  2  ({file})"),
            "Set it with: riff workers limit N",
        ),
        (
            &["workers", "mcp"],
            format!("workers.mcp  riff  ({file})"),
            "Change it with: riff workers mcp add NAME, or riff workers mcp remove NAME",
        ),
        (
            &["update", "--auto", "on"],
            format!("update.auto  true  ({file})"),
            "Turn it off with: riff update --auto off",
        ),
        (
            &["update", "--auto"],
            format!("update.auto  true  ({file})"),
            "Turn it off with: riff update --auto off",
        ),
    ];
    for (args, fact, hint) in cases {
        let out = stdout(riff(&env, isolated::DEAD_SERVER, dir, None, args)).await;
        assert_eq!(out, format!("{fact}\n{hint}\n"), "{args:?}");
    }
}
