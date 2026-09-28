//! `riff who` for people, with the styles of `riff tail`
//! (01M3MEW73CDSJDSKX32XW80WZH), and its `--color`
//! (01M3MEW75WC7Y4M1BKQ7SXRPNR), against a real server.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use riff::style;
use riff_core::name::SessionUri;

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

/// `riff ARGS` in `dir` through a pipe, as the agent session `session`
/// of mike, or as the person brett.
fn riff(server: &str, dir: &Path, session: Option<&str>, args: &[&str]) -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin("riff"));
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("XDG_RUNTIME_DIR", dir)
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match session {
        Some(id) => cmd.env("RIFF_USER", "mike").env("RIFF_SESSION", id),
        None => cmd.env("RIFF_USER", "brett"),
    };
    cmd
}

/// The stdout of `cmd`, after its exit.
async fn output(mut cmd: Command) -> String {
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

/// The URI of the agent session `id` of mike.
fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/{THREAD}?session={id}")
        .parse()
        .unwrap()
}

/// `name` in the style of the session `id`.
fn in_color(id: &str, name: &str) -> String {
    style::styled(style::session(&uri(id)), name)
}

#[tokio::test(flavor = "multi_thread")]
async fn who_prints_each_session_in_the_color_of_tail() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();

    // Follow the thread, then post from two sessions.
    let mut tail = riff(&server, dir, None, &["tail", THREAD, "--color", "always"])
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    for id in ["a1", "b2"] {
        let post = riff(&server, dir, Some(id), &["post", "-t", THREAD, "hello"]);
        output(post).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    tail.kill().unwrap();
    let mut tailed = String::new();
    tail.stdout
        .take()
        .unwrap()
        .read_to_string(&mut tailed)
        .unwrap();
    tail.wait().unwrap();

    let who = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    assert_ne!(
        style::session(&uri("a1")),
        style::session(&uri("b2")),
        "pick two sessions with two colors"
    );
    for (id, name) in [
        ("a1", "mike@pangolin:riff (a1)"),
        ("b2", "mike@pangolin:riff (b2)"),
    ] {
        let name = in_color(id, name);
        assert!(tailed.contains(&name), "{tailed:?}");
        assert!(who.contains(&name), "{who:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pipe_gets_no_color_unless_asked() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    output(riff(&server, dir, Some("a1"), &["read"])).await;

    let auto = output(riff(&server, dir, None, &["who"])).await;
    assert!(
        auto.contains("mike@pangolin:riff (a1)  idle 0s  lead  riff://"),
        "{auto}"
    );
    assert!(!auto.contains('\x1b'), "{auto:?}");

    let mut never = riff(&server, dir, None, &["who", "--color", "never"]);
    never.env("CLICOLOR_FORCE", "1");
    let never = output(never).await;
    assert!(!never.contains('\x1b'), "{never:?}");

    let mut no_color = riff(&server, dir, None, &["who"]);
    no_color.env("NO_COLOR", "1");
    let no_color = output(no_color).await;
    assert!(!no_color.contains('\x1b'), "{no_color:?}");

    let always = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    assert!(always.contains("\x1b["), "{always:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_status_is_red() {
    let server = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let blocked = ["status", "--blocked", "waits for a review", "merge"];
    output(riff(&server, dir, Some("a1"), &blocked)).await;

    let who = output(riff(&server, dir, None, &["who", "--color", "always"])).await;
    let red = style::ERROR;
    let line = "blocked 0s ago: waits for a review (step: merge)";
    assert!(
        who.contains(&format!("       {red}{line}{red:#}\n")),
        "{who:?}"
    );
}

/// Each `riff who` command in the `sh` blocks of How It Works is real,
/// and the book shows `--color`.
#[test]
fn the_book_shows_real_who_commands() {
    let page = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let mut in_sh = false;
    let mut commands = Vec::new();
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff who" || line.starts_with("riff who ")) {
            commands.push(line.split(['|', '>']).next().unwrap().trim().to_owned());
        }
    }
    assert!(
        commands.iter().any(|c| c == "riff who --color never"),
        "{commands:?}"
    );
    for command in commands {
        assert_cmd::Command::cargo_bin("riff")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
