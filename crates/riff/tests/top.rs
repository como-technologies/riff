//! `riff top`: a live table of each session, its item and its status
//! (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
//! (01M3NB589WMPRSAR43BSG9SP41).

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// The path of each call that the server gets.
type Calls = Arc<Mutex<Vec<String>>>;

async fn start_server() -> (String, Calls) {
    let calls = Calls::default();
    let seen = calls.clone();
    let router = riff_server::router().layer(axum::middleware::map_request(
        move |r: axum::extract::Request| {
            seen.lock().unwrap().push(r.uri().path().to_owned());
            async move { r }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), calls)
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

/// A dir for `PATH` with `git`, a fake `tmux` that lists one worker
/// pane of the session `b2`, and a fake `gh` when `gh` is true.
fn bin(gh: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    script(dir.path(), "tmux", "echo '%3 b2'");
    if gh {
        script(
            dir.path(),
            "gh",
            r#"echo '[{"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 3"}}]'"#,
        );
    }
    dir
}

fn script(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `riff ARGS` in `dir` through a pipe, as the agent session `session`
/// of mike, or as the person brett, with `path` as `PATH`.
fn riff(server: &str, dir: &Path, session: Option<&str>, path: &Path, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("PATH", path)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    String::from_utf8(out.stdout).unwrap()
}

/// A lead `a1`, a worker `b2` with a claim and a status, and a blocked
/// session `c3`, all of mike.
async fn three_sessions(server: &str, dir: &Path, path: &Path) {
    for (id, args) in [
        ("a1", &["read"][..]),
        ("a1", &["resume"][..]),
        ("b2", &["claim", "issue-12"][..]),
        ("b2", &["status", "tests"][..]),
        (
            "c3",
            &["status", "--blocked", "waits for a review", "merge"][..],
        ),
    ] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
}

/// The rows of the table, after the head.
fn rows(top: &str) -> Vec<String> {
    top.lines()
        .skip_while(|l| !l.starts_with("SESSION"))
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn top_once_prints_a_row_for_each_session_blocked_first() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(top.starts_with("The riff is running.\n"), "{top}");
    assert!(top.contains("\nWave 3: #12 b2\n"), "{top}");
    let rows = rows(&top);
    assert_eq!(rows.len(), 3, "{top}");
    assert!(rows[0].starts_with("mike@pangolin (c3)"), "{top}");
    assert!(
        rows[0].contains("blocked 0s: waits for a review (step: merge)"),
        "{top}"
    );
    assert!(rows[1].starts_with("mike@pangolin (a1)"), "{top}");
    assert!(rows[1].contains(" lead "), "{top}");
    assert!(rows[2].starts_with("mike@pangolin (b2)"), "{top}");
    assert!(rows[2].contains(" worker "), "{top}");
    assert!(rows[2].contains("issue-12 Show the wave"), "{top}");
    assert!(rows[2].ends_with("0s tests"), "{top}");
    for row in &rows {
        assert!(row.contains(" idle ") || row.contains(" live "), "{top}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn top_never_prints_color_to_a_pipe() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let pipe = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(!pipe.contains('\x1b'), "{pipe:?}");
    let mut never = riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once", "--color", "never"],
    );
    never.env("CLICOLOR_FORCE", "1");
    let never = output(never).await;
    assert!(!never.contains('\x1b'), "{never:?}");
    let always = ["top", "--once", "--color", "always"];
    let always = output(riff(&server, dir, Some("a1"), bin.path(), &always)).await;
    let red = riff::style::ERROR;
    assert!(always.contains(&format!("{red}blocked 0s")), "{always:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_gh_the_row_still_prints() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    three_sessions(&server, dir, bin.path()).await;

    let top = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(!top.contains("Wave 3"), "{top}");
    let rows = rows(&top);
    assert_eq!(rows.len(), 3, "{top}");
    assert!(rows[2].contains("issue-12  "), "{top}");
    assert!(!rows[2].contains("Show the wave"), "{top}");
}

#[tokio::test(flavor = "multi_thread")]
async fn top_makes_only_read_calls() {
    let (server, calls) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    calls.lock().unwrap().clear();

    // As the person and as an agent session, once and live.
    output(riff(&server, dir, None, bin.path(), &["top", "--once"])).await;
    output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    let mut live = riff(&server, dir, None, bin.path(), &["top"])
        .spawn()
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    live.kill().unwrap();
    live.wait().unwrap();

    let calls = calls.lock().unwrap().clone();
    assert!(
        calls.iter().filter(|c| *c == "/v1/who").count() >= 4,
        "{calls:?}"
    );
    let reads = ["/v1/riff", "/v1/who", "/v1/tail"];
    for call in &calls {
        assert!(reads.contains(&call.as_str()), "{call} in {calls:?}");
    }
}

/// Each `riff top` command in the `sh` blocks of How It Works is real.
#[test]
fn the_book_shows_real_top_commands() {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let mut in_sh = false;
    let mut commands = Vec::new();
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff top" || line.starts_with("riff top ")) {
            commands.push(line.split(['|', '>']).next().unwrap().trim().to_owned());
        }
    }
    for want in ["riff top", "riff top --once"] {
        assert!(commands.iter().any(|c| c == want), "{want}: {commands:?}");
    }
    for command in commands {
        Isolated::shared()
            .assert_riff()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
