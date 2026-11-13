//! `riff ps` (#1610): the riff processes of a host as a tree.
//!
//! One test runs the real daemon of a `RIFF_HOME`, which reads the real
//! `/proc`. The others use a fake daemon on the socket of a session, a
//! daemon that never answers (#1923), or no daemon.

use std::process::Stdio;
use std::time::Duration;

use serde_json::json;

use crate::common;
use crate::daemon::{Here, stop};

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The line of `shown` that starts with the process ID `pid`.
fn line_of(shown: &str, pid: u32) -> &str {
    shown
        .lines()
        .find(|line| {
            let line = line.trim_start_matches([' ', '│', '├', '└', '─']);
            line.starts_with(&format!("{pid}  "))
        })
        .unwrap_or_else(|| panic!("no line for {pid}: {shown}"))
}

/// 01M6QBXNMEBANX439DF7J1PQ3S (#1610): the daemon reads the processes of
/// the host. `riff ps` shows the daemon, the containers, and the riff
/// command that asks, with its parent up to the first process that is not
/// riff: here the test.
#[tokio::test(flavor = "multi_thread")]
async fn riff_ps_shows_the_daemon_and_a_riff_command_with_its_parent() {
    let here = Here::new();
    let daemon = here.daemon("daemon.out").await;
    let mut cmd = here.command(false, &["ps"]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().unwrap();
    let ps_pid = child.id();
    let out = tokio::task::spawn_blocking(move || child.wait_with_output().unwrap())
        .await
        .unwrap();
    let shown = text(&out.stdout);
    assert!(out.status.success(), "{shown}{}", text(&out.stderr));

    // The daemon is the first process, under its heading.
    let mut lines = shown.lines();
    assert_eq!(lines.next(), Some("Daemon:"), "{shown}");
    let first = lines.next().unwrap();
    assert!(
        first.starts_with(&format!("  {}  ", daemon.id())) && first.ends_with("riff daemon"),
        "{shown}"
    );
    assert!(shown.contains("\nContainers: none.\n"), "{shown}");
    assert!(shown.contains("\nOutside a container:\n"), "{shown}");

    // `riff ps` is a riff command outside a container. Its parent is the
    // test: the first process that is not riff.
    let own = line_of(&shown, ps_pid);
    assert!(own.ends_with("riff ps"), "{shown}");
    assert!(own.contains("└─ ") || own.contains("├─ "), "{shown}");
    let test = line_of(&shown, std::process::id());
    assert!(!test.contains("└─") && !test.contains("├─"), "{shown}");
    stop(daemon).await;
}

/// 01M6QBXNY8GM1DRN4RR9TPSEZH (#1610): with no daemon, `riff ps` says so,
/// shows the state of the unit and how to start the daemon, and exits
/// with 1.
#[tokio::test]
async fn riff_ps_with_no_daemon_says_so_and_how_to_start_it() {
    let here = Here::new();
    let out = here.run(false, &["ps"]).await;
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    let stderr = text(&out.stderr);
    let socket = here.daemon_dir().join("riff.sock");
    assert!(
        stderr.contains(&format!("No riff daemon answers on {}", socket.display())),
        "{stderr}"
    );
    assert!(
        stderr.contains("The unit riff.service is inactive."),
        "{stderr}"
    );
    assert!(
        stderr.contains("Start it: riff daemon install."),
        "{stderr}"
    );
}

/// The time limit of `riff ps` and `riff daemon status` in a test, in
/// milliseconds ([`riff::daemon::ANSWER_VAR`]).
pub(crate) const LIMIT_MS: &str = "300";

/// 01M7AZENPBNJ530XH0TNZPNSM9 (#1923): a daemon that takes the
/// connection and never answers. `riff ps` ends at the time limit, says
/// that the daemon gave no answer in that time, shows the state of the
/// unit, and exits with 1.
#[tokio::test]
async fn riff_ps_of_a_daemon_that_never_answers_ends_at_the_limit() {
    let here = Here::new();
    std::fs::create_dir_all(here.daemon_dir()).unwrap();
    common::silent_socket(&here.daemon_dir().join("riff.sock"));
    let mut cmd = here.command(false, &["ps"]);
    cmd.env(riff::daemon::ANSWER_VAR, LIMIT_MS);
    let most = Duration::from_secs(30);
    let (out, took) = tokio::task::spawn_blocking(move || common::output_in(cmd, most))
        .await
        .unwrap();
    assert!(took >= Duration::from_millis(300), "{took:?}");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("The riff daemon gave no answer in 0.3 s.\n"),
        "{stderr}"
    );
    assert!(
        stderr.contains("The unit riff.service is inactive."),
        "{stderr}"
    );
}

/// 01M7AZENPBNJ530XH0TNZPNSM9 (#1923): in the container of the lead, a
/// daemon that takes the connection on the socket of the session and
/// never answers. `riff ps` ends at the time limit, says so, and exits
/// with 1.
#[tokio::test]
async fn riff_ps_in_the_lead_container_ends_at_the_limit() {
    let here = Here::new();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("riff.sock");
    common::silent_socket(&socket);
    let mut cmd = here.command(false, &["ps"]);
    cmd.env("RIFF_SOCKET", &socket)
        .env(riff::daemon::ANSWER_VAR, LIMIT_MS);
    let most = Duration::from_secs(30);
    let (out, _) = tokio::task::spawn_blocking(move || common::output_in(cmd, most))
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(
        text(&out.stderr),
        "The riff daemon gave no answer in 0.3 s.\n"
    );
}

fn answer() -> serde_json::Value {
    json!({
        "daemon": 812,
        "host": [
            {"pid": 812, "ppid": 1, "secs": 259_200, "args": "riff daemon"},
            {"pid": 3001, "ppid": 900, "secs": 7_200, "args": "watch -n 5 riff wave list"},
            {"pid": 3002, "ppid": 3001, "secs": 5, "args": "riff wave list", "calls": 12, "points": 24},
        ],
        "containers": [
            {
                "name": "riff-lead-o-r", "kind": "lead", "item": "lead", "session": "s0",
                "procs": [{"pid": 1, "ppid": 0, "secs": 600, "args": "riff relay --terminal -- claude"}],
            },
            {
                "name": "riff-verify-issue-12-2b3c4d5e", "kind": "verify",
                "item": "verify-issue-12", "session": "s2", "procs": [],
                "error": "podman does not answer",
            },
        ],
    })
}

/// 01M6QBXNQC6X7NJVR03M92V0Q8, 01M6QBXNTMH7B2SNXBHAFKCN81 (#1610): in the
/// container of the lead, `riff ps` asks the daemon on the socket of the
/// session, and draws the answer. The line of a process that made calls
/// ends with them.
#[tokio::test(flavor = "multi_thread")]
async fn riff_ps_in_the_lead_container_draws_the_answer_of_the_daemon() {
    let here = Here::new();
    let reply = answer();
    let (_dir, socket, asked) = common::session_socket(move |_| Ok(reply.clone()));
    let mut cmd = here.command(false, &["ps"]);
    cmd.env("RIFF_SOCKET", &socket);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let shown = text(&out.stdout);
    assert!(out.status.success(), "{shown}{}", text(&out.stderr));
    assert_eq!(common::call_names(&asked), ["ps"]);
    assert_eq!(
        shown,
        "Daemon:\n\
         \x20 812  3d  riff daemon\n\
         Containers:\n\
         \x20 riff-lead-o-r  lead  session s0\n\
         \x20   1  10m  riff relay --terminal -- claude\n\
         \x20 riff-verify-issue-12-2b3c4d5e  verify  verify-issue-12  session s2\n\
         \x20   No list of processes: podman does not answer\n\
         Outside a container:\n\
         \x20 3001  2h  watch -n 5 riff wave list\n\
         \x20 └─ 3002  5s  riff wave list  GitHub, last hour: 12 calls, 24 points\n"
    );
}

/// 01M6QBXNQC6X7NJVR03M92V0Q8 (#1610): a session that is not the lead
/// gets the refusal of the daemon, and `riff ps` exits with 1.
#[tokio::test(flavor = "multi_thread")]
async fn riff_ps_in_a_worker_container_shows_the_refusal() {
    let here = Here::new();
    let (_dir, socket, _asked) =
        common::session_socket(|_| Err(riff::text::DAEMON_PS_ONLY_LEAD.to_owned()));
    let mut cmd = here.command(false, &["ps"]);
    cmd.env("RIFF_SOCKET", &socket);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert!(
        text(&out.stderr).contains(riff::text::DAEMON_PS_ONLY_LEAD),
        "{}",
        text(&out.stderr)
    );
}
