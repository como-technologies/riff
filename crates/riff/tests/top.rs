//! `riff top`: a live table of each session, its item and its status
//! (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
//! (01M3NB589WMPRSAR43BSG9SP41).

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use riff::api::Api;
use riff_core::name::SessionUri;

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
/// pane of the session `c3` on this machine, and a fake `gh` when `gh`
/// is true. The pane does not make `c3` a worker: only the server says
/// who is a worker (01M3NT4M159EHN5W8JRTQ417N4).
fn bin(gh: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    script(dir.path(), "tmux", "echo '%3 c3'");
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
/// session `c3`, all of mike. `b2` registers as a worker on the host
/// thelio. Each command runs on the host pangolin.
async fn three_sessions(server: &str, dir: &Path, path: &Path) {
    for (id, args) in [("a1", &["read"][..]), ("a1", &["resume"][..])] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
    let b2: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    Api::new(server).register_as(&b2, true).await.unwrap();
    for (id, args) in [
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
        .skip_while(|l| !l.starts_with("WHO"))
        .skip(1)
        .map(str::to_owned)
        .collect()
}

/// The session rows of the table.
fn session_rows(top: &str) -> Vec<String> {
    rows(top)
        .into_iter()
        .filter(|r| r.contains("─ ") && (r.contains(" live ") || r.contains(" idle ")))
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
    assert_eq!(rows.len(), 6, "{top}");
    assert!(rows[0].starts_with("mike "), "{top}");
    assert!(rows[0].contains(" last seen "), "{top}");
    assert_eq!(rows[1], "├─ pangolin", "{top}");
    assert!(rows[2].starts_with("│  ├─ c3 "), "{top}");
    assert!(
        rows[2].contains("blocked 0s: waits for a review (step: merge)"),
        "{top}"
    );
    assert!(
        !rows[2].contains("worker"),
        "a local pane is no worker: {top}"
    );
    assert!(rows[3].starts_with("│  └─ a1 "), "{top}");
    assert!(rows[3].contains(" lead "), "{top}");
    assert_eq!(rows[4], "└─ thelio", "{top}");
    assert!(rows[5].starts_with("   └─ b2 "), "{top}");
    assert!(rows[5].contains(" worker "), "{top}");
    assert!(rows[5].contains("issue-12 Show the wave"), "{top}");
    assert!(rows[5].ends_with("0s tests"), "{top}");
    for row in session_rows(&top) {
        assert!(row.contains(" idle ") || row.contains(" live "), "{top}");
        assert!(!row.contains("owner"), "{top}");
    }
}

/// A worker that registered on thelio shows `worker` in `riff top` and
/// `riff who` on pangolin (01M3NT4M159EHN5W8JRTQ417N4).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_of_another_host_shows_worker() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let b2 = session_rows(&top)
        .into_iter()
        .find(|r| r.contains("─ b2 "))
        .unwrap();
    assert!(b2.contains(" worker "), "{top}");

    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let line = |id: &str| {
        who.lines()
            .find(|l| l.contains(&format!("({id})")))
            .unwrap_or_else(|| panic!("{id}: {who}"))
            .to_owned()
    };
    assert!(line("b2").starts_with("mike@thelio:riff (b2)"), "{who}");
    assert!(line("b2").contains("  worker  "), "{who}");
    assert!(line("b2").contains("  issue-12"), "{who}");
    assert!(!line("c3").contains("worker"), "{who}");
    assert!(line("a1").contains("  you lead"), "{who}");
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
    assert_eq!(rows.len(), 6, "{top}");
    assert!(rows[5].contains("issue-12  "), "{top}");
    assert!(!rows[5].contains("Show the wave"), "{top}");
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

/// The STATUS cell of the session row `id`: the text after its ITEM
/// cell `item`.
fn status_of(top: &str, id: &str, item: &str) -> String {
    let row = session_rows(top)
        .into_iter()
        .find(|r| r.contains(&format!("─ {id} ")))
        .unwrap_or_else(|| panic!("{id}: {top}"));
    let (_, status) = row
        .split_once(&format!("  {item}  "))
        .unwrap_or_else(|| panic!("{item} in {row}"));
    status.trim_start().to_owned()
}

/// A pause is a change of the state of each session: `riff top` shows
/// `paused` first, and the step from before the pause as stale
/// (01M3Q551YHYZBFV2NDS1QCYXCD, 01M3Q555KC1RKNEC4ZA9HQYJG2).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_shows_paused_and_the_old_step_as_stale() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    output(riff(&server, dir, Some("a1"), bin.path(), &["pause"])).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let b2 = status_of(&top, "b2", "issue-12 Show the wave");
    assert!(b2.starts_with("paused  stale "), "{top}");
    assert!(b2.ends_with(": tests"), "{top}");
    let c3 = status_of(&top, "c3", "-");
    assert!(c3.starts_with("paused  stale "), "{top}");
    assert!(
        c3.ends_with(": blocked: waits for a review (step: merge)"),
        "{top}"
    );
    let rows = rows(&top);
    assert!(
        rows[2].starts_with("│  ├─ a1 "),
        "a stale block does not come first: {top}"
    );
}

/// A worker that releases its claim shows `idle` with its time, not its
/// old step (01M3Q551WCMPQRCNJ8FXQEBFY4, 01M3Q555KC1RKNEC4ZA9HQYJG2).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_no_claim_shows_idle_not_its_old_step() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let release = ["release", "issue-12"];
    output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let b2 = status_of(&top, "b2", "-");
    assert!(b2.starts_with("idle "), "{top}");
    assert!(b2.contains("  stale "), "{top}");
    assert!(b2.ends_with(": tests"), "{top}");
    assert!(top.contains("\nWave 3: #12 free\n"), "{top}");
}

/// The lead row shows the current wave and its open items, with no
/// status call (01M3Q555KC1RKNEC4ZA9HQYJG2).
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_row_shows_the_wave_with_no_status_call() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert_eq!(status_of(&top, "a1", "-"), "Wave 3: #12", "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let a1 = who.lines().skip_while(|l| !l.contains("(a1)")).nth(1);
    assert!(
        a1.is_none_or(|l| !l.contains("status")),
        "a1 set no status: {who}"
    );
}
