//! The lead and its workers in tmux (01M3JD390F49HZSKEJ3VACX0ZA to
//! 01M3JD39BASN1GNJTZXXKBCNZ9). A fake `tmux` on `PATH` writes each call
//! to a log, and keeps the marks of the panes and windows in files.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::terminal::{self, Program, Tmux};
use riff_core::name::SessionUri;

const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
case "$1" in
  list-panes) cat "$dir/panes" 2>/dev/null ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message) echo "@0" ;;
  new-window) echo "@7" ;;
  split-window) echo "%$(grep -c '^split-window' "$dir/log")" ;;
  set-option)
    case "$2" in
      -p) echo "$6" >> "$dir/panes" ;;
      -w) echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
esac
exit 0
"#;

/// A directory with the fake `tmux`.
fn fake_tmux() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let tmux = dir.path().join("tmux");
    std::fs::write(&tmux, FAKE_TMUX).unwrap();
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

fn log(fake: &Path) -> String {
    std::fs::read_to_string(fake.join("log")).unwrap_or_default()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository `main` with a linked worktree `wt`. Returns both paths.
fn repository(root: &Path) -> (PathBuf, PathBuf) {
    let main = root.join("main");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "x"]);
    git(&main, &["worktree", "add", "-q", "../wt"]);
    (
        std::fs::canonicalize(&main).unwrap(),
        std::fs::canonicalize(root.join("wt")).unwrap(),
    )
}

/// `riff workers start` in `dir`, with the fake `tmux` first on `PATH`.
fn workers(fake: &Path, dir: &Path, args: &[&str], in_tmux: bool) -> std::process::Output {
    let path = format!("{}:{}", fake.display(), std::env::var("PATH").unwrap());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_riff"));
    cmd.args(["workers", "start"])
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("RIFF_SERVER", "http://riff.test:7878");
    if in_tmux {
        cmd.env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0");
    } else {
        cmd.env_remove("TMUX").env_remove("TMUX_PANE");
    }
    cmd.output().unwrap()
}

#[test]
fn workers_start_opens_one_window_with_a_pane_for_each_worker() {
    let fake = fake_tmux();
    let root = tempfile::tempdir().unwrap();
    let (main, wt) = repository(root.path());
    let out = workers(fake.path(), &wt, &["3"], true);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout.contains("Started 3 workers in")
            && stdout.contains("tmux select-window -t riff-workers"),
        "{stdout}"
    );

    let log = log(fake.path());
    let calls: Vec<&str> = log.lines().collect();
    let dir = main.display();
    let pane = format!(
        "-d -c {dir} -P -F #{{pane_id}} -e RIFF_SERVER=http://riff.test:7878 -e RIFF_WORKER=1 \
         'claude' 'Join the riff.'"
    );
    assert_eq!(
        calls,
        [
            "list-windows -t %0 -F #{window_id} #{@riff}".to_owned(),
            "display-message -p -t %0 #{window_id}".to_owned(),
            format!(
                "new-window -a -t @0 -n riff-workers -d -c {dir} -P -F #{{window_id}} \
                 -e RIFF_SERVER=http://riff.test:7878 -e RIFF_WORKER=1 'claude' 'Join the riff.'"
            ),
            "set-option -w -t @7 @riff workers".to_owned(),
            format!("split-window -t @7 {pane}"),
            "select-layout -t @7 tiled".to_owned(),
            format!("split-window -t @7 {pane}"),
            "select-layout -t @7 tiled".to_owned(),
        ]
    );
    // A worker has no Remote Control (01M3JD394YFA3TQRE3E72ZER4Z).
    assert!(!log.contains("remote-control"), "{log}");
}

#[test]
fn a_second_start_adds_panes_to_the_same_window() {
    let fake = fake_tmux();
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    assert!(workers(fake.path(), &main, &["1"], true).status.success());
    let out = workers(fake.path(), &main, &["2", "--claude", "/opt/claude"], true);
    assert!(out.status.success(), "{out:?}");
    let log = log(fake.path());
    assert_eq!(log.matches("new-window").count(), 1, "{log}");
    assert_eq!(log.matches("split-window -t @7").count(), 2, "{log}");
    assert!(log.contains("'/opt/claude' 'Join the riff.'"), "{log}");
}

#[test]
fn outside_tmux_workers_start_starts_nothing() {
    let fake = fake_tmux();
    let root = tempfile::tempdir().unwrap();
    let (main, _) = repository(root.path());
    let out = workers(fake.path(), &main, &["3"], false);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("needs tmux") && stderr.contains("started nothing"),
        "{stderr}"
    );
    assert_eq!(log(fake.path()), "");
}

#[test]
fn workers_start_needs_a_count_of_one_or_more() {
    let fake = fake_tmux();
    let out = workers(fake.path(), Path::new("/"), &["0"], true);
    assert!(!out.status.success());
    assert_eq!(log(fake.path()), "");
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_lead_gets_one_tail_pane() {
    let api = start_server().await;
    let lead = uri("riff://mike@pangolin/como-technologies/riff?session=a1");
    let worker = uri("riff://mike@pangolin/como-technologies/riff?session=a2");
    api.register(&lead).await.unwrap();
    api.register(&worker).await.unwrap();
    let fake = fake_tmux();
    let tmux = Tmux::new(fake.path().join("tmux"), "%0");
    let tail = Program::tail("/bin/riff".as_ref(), "/src/riff".as_ref(), api.base());

    // A session that is not the lead gets no pane, and calls no tmux.
    assert!(
        !terminal::tail_beside_lead(&api, &worker, &tmux, &tail)
            .await
            .unwrap()
    );
    assert_eq!(log(fake.path()), "");

    assert!(
        terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
    // A restart, a /clear or a resume finds the marked pane.
    assert!(
        !terminal::tail_beside_lead(&api, &lead, &tmux, &tail)
            .await
            .unwrap()
    );
    let log = log(fake.path());
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        [
            "list-panes -t %0 -F #{@riff}".to_owned(),
            format!(
                "split-window -h -t %0 -d -c /src/riff -P -F #{{pane_id}} -e RIFF_SERVER={} \
                 '/bin/riff' tail",
                api.base()
            ),
            "set-option -p -t %1 @riff tail".to_owned(),
            "list-panes -t %0 -F #{@riff}".to_owned(),
        ]
    );
}

/// The lead runs `riff mcp` in tmux: `riff mcp` adds the tail pane.
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_of_the_lead_adds_the_tail_pane_in_tmux() {
    let api = start_server().await;
    let fake = fake_tmux();
    let run = tempfile::tempdir().unwrap();
    let (main, _) = repository(run.path());
    let path = format!(
        "{}:{}",
        fake.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut mcp = tokio::process::Command::new(env!("CARGO_BIN_EXE_riff"))
        .arg("mcp")
        .current_dir(&main)
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-1000/default,1,0")
        .env("TMUX_PANE", "%0")
        .env("XDG_RUNTIME_DIR", run.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !log(fake.path()).contains("@riff tail") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{}",
            log(fake.path())
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = log(fake.path());
    assert_eq!(log.matches("split-window -h").count(), 1, "{log}");
    assert!(log.contains("' tail"), "{log}");
    mcp.kill().await.unwrap();
}

#[test]
fn the_book_has_a_how_to_for_each_step() {
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md");
    let book = std::fs::read_to_string(book).unwrap();
    let part = &book[book
        .find("## Run the lead and its workers in tmux")
        .unwrap()..];
    for (heading, command) in [
        ("### Start the lead in tmux", "claude --remote-control"),
        ("### Start workers", "riff workers start 3"),
        ("### Start workers", "riff workers start 1 --claude "),
        (
            "### Take over a worker",
            "tmux select-window -t riff-workers",
        ),
    ] {
        let how = &part[part.find(heading).unwrap()..];
        let next = how[4..].find("\n### ").map_or(how.len(), |n| n + 4);
        assert!(
            how[..next].contains("```sh\n") && how[..next].contains(command),
            "{heading} has no {command:?}"
        );
    }
    let help = Command::new(env!("CARGO_BIN_EXE_riff"))
        .args(["workers", "start", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.contains("--claude <CLAUDE>") && help.contains("<COUNT>"),
        "{help}"
    );
}
