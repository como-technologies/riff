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

/// Opens a watch for the session `id` of mike on `host`, so that the
/// session is live until the test ends.
async fn live(server: &str, host: &str, id: &str) {
    let uri: SessionUri = format!("riff://mike@{host}/como-technologies/riff?session={id}")
        .parse()
        .unwrap();
    let api = Api::new(server);
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _watch = api.watch(&uri).await.unwrap();
        open.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    opened.await.unwrap();
}

/// A lead `a1`, a worker `b2` with a claim and a status, and a blocked
/// session `c3`, all of mike and each live. `b2` registers as a worker
/// on the host thelio. Each command runs on the host pangolin.
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
    for (host, id) in [("pangolin", "a1"), ("thelio", "b2"), ("pangolin", "c3")] {
        live(server, host, id).await;
    }
}

/// The lines of the tree, after the header and the board.
fn rows(top: &str) -> Vec<String> {
    top.rsplit("\n\n")
        .next()
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The lines of the session `id`: its first line, then the lines under
/// it.
fn session(top: &str, id: &str) -> Vec<String> {
    let rows = rows(top);
    let head = rows
        .iter()
        .position(|r| r.contains(&format!("─ {id}  ")))
        .unwrap_or_else(|| panic!("{id}: {top}"));
    let under = rows[head + 1..]
        .iter()
        .take_while(|r| !r.contains("─ ") && (r.starts_with(' ') || r.starts_with('│')));
    std::iter::once(&rows[head]).chain(under).cloned().collect()
}

/// The detail lines of the session `id`, with no lead-in.
fn detail(top: &str, id: &str) -> Vec<String> {
    session(top, id)
        .into_iter()
        .skip(1)
        .map(|l| l.trim_start_matches([' ', '│']).to_owned())
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
    assert!(top.starts_with("riff   running\n"), "{top}");
    assert!(top.contains("\n\nWave 3\n  claimed: #12\n\n"), "{top}");
    let rows = rows(&top);
    assert_eq!(
        rows,
        [
            "mike  online",
            "├─ pangolin",
            "│  ├─ c3  blocked",
            "│  │    waits for a review (step: merge, 0s ago)",
            "│  └─ a1  lead  idle",
            "│       ready for work for 0s",
            "└─ thelio",
            "   └─ b2  worker  busy",
            "        working on #12 Show the wave",
            "        0s ago: tests",
        ],
        "a local pane is no worker; blocked comes first: {top}"
    );
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
    assert!(session(&top, "b2")[0].contains("  worker  "), "{top}");

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
    assert!(line("b2").contains("  busy  "), "{who}");
    assert!(line("b2").contains("  working on #12  "), "{who}");
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
    assert!(
        always.contains(&format!("{red}blocked{red:#}")),
        "{always:?}"
    );
    let green = riff::style::GOOD;
    assert!(
        always.contains(&format!("{green}busy{green:#}")),
        "{always:?}"
    );
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
    assert_eq!(
        detail(&top, "b2"),
        ["working on #12", "0s ago: tests"],
        "{top}"
    );
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

/// A pause makes each live session `paused`, with the step it stopped
/// at. A stale block does not come first (01M3QB6CJ1XCQG5B1BVR8AF3B4).
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_shows_paused_and_the_step() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    output(riff(&server, dir, Some("a1"), bin.path(), &["pause"])).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    for id in ["a1", "b2", "c3"] {
        assert!(session(&top, id)[0].ends_with("  paused"), "{top}");
    }
    assert_eq!(
        detail(&top, "b2"),
        ["working on #12 Show the wave", "stopped at: tests"],
        "{top}"
    );
    assert_eq!(detail(&top, "c3"), ["stopped at: merge"], "{top}");
    let rows = rows(&top);
    assert!(
        rows[2].starts_with("│  ├─ a1  "),
        "a stale block does not come first: {top}"
    );
}

/// A worker that releases its claim is `idle`, ready for work, with no
/// old step (01M3Q551WCMPQRCNJ8FXQEBFY4, 01M3QB6CJ1XCQG5B1BVR8AF3B4).
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
    assert!(session(&top, "b2")[0].ends_with("  worker  idle"), "{top}");
    assert_eq!(detail(&top, "b2"), ["ready for work for 0s"], "{top}");
    assert!(top.contains("\nWave 3\n  free: #12\n"), "{top}");
}

/// A session with no claim and no status is `idle` in `riff top` and in
/// `riff who`, with no status call (01M3QB6CJ1XCQG5B1BVR8AF3B4).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_no_status_is_idle() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert_eq!(detail(&top, "a1"), ["ready for work for 0s"], "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let a1 = who.lines().find(|l| l.contains("(a1)")).unwrap();
    assert!(a1.contains("  idle  "), "{who}");
    assert!(a1.ends_with("  ready for work for 0s"), "{who}");
}

/// With 12 sessions, long titles and long statuses, `riff top --once`
/// in a pipe fits in 80 columns: riff cuts each wider line with `…`
/// (01M3QA8EZHX5B8C9CKF8Q3154X).
#[tokio::test(flavor = "multi_thread")]
async fn twelve_sessions_fit_in_80_columns() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    let long = "The owner check does not drop an owner that has a live session on another host";
    script(
        bin.path(),
        "gh",
        &format!(
            r#"echo '[{{"number": 12, "title": "{long}", "milestone": {{"title": "Wave 3: A long name"}}}}, {{"number": 13, "title": "{long}", "milestone": {{"title": "Wave 3: A long name"}}}}]'"#
        ),
    );
    output(riff(&server, dir, Some("s00"), bin.path(), &["resume"])).await;
    for n in 0..12 {
        let id = format!("s{n:02}");
        let uri: SessionUri = format!("riff://mike@thelio/como-technologies/riff?session={id}")
            .parse()
            .unwrap();
        Api::new(&server)
            .register_as(&uri, n % 2 == 0)
            .await
            .unwrap();
        let claim = match n {
            0 => Some("issue-12"),
            3 => Some("verify-issue-12"),
            6 => Some("issue-13"),
            _ => None,
        };
        if let Some(claim) = claim {
            output(riff(&server, dir, Some(&id), bin.path(), &["claim", claim])).await;
        }
        live(&server, "thelio", &id).await;
        let status = [
            "status",
            "--blocked",
            long,
            "waits for a verify of the pull request",
        ];
        let step = ["status", long];
        let args: &[&str] = if n % 4 == 0 { &status } else { &step };
        output(riff(&server, dir, Some(&id), bin.path(), args)).await;
    }

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("s00"), bin.path(), &top)).await;
    for line in top.lines() {
        assert!(line.chars().count() <= 80, "{line:?} in\n{top}");
    }
    assert!(top.lines().any(|l| l.ends_with('…')), "{top}");
    assert!(top.contains("\n  claimed: #13\n  verify: #12\n"), "{top}");
    assert!(
        top.contains("│    working on #12 The owner check does not"),
        "{top}"
    );
    for n in 0..12 {
        assert!(!session(&top, &format!("s{n:02}")).is_empty(), "{top}");
    }
}
