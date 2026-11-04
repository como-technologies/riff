//! The `gh` of a container session counts its calls (#1582,
//! 01M6JGF7265VX2SDXZ4EJAGXVB).
//!
//! The daemon binds the binary of riff at `/usr/local/bin/gh`. The test
//! links the binary under test as `gh`, and runs it with a fake real `gh`
//! (`RIFF_GH_REAL`) and a fake daemon on the socket of a session.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use isolated::Isolated;

/// A fake daemon on a socket: it keeps the first line of each connection.
struct Daemon {
    socket: PathBuf,
    lines: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl Daemon {
    fn start(dir: &Path) -> Daemon {
        let socket = dir.join("riff.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let kept = lines.clone();
        std::thread::spawn(move || {
            while let Ok((conn, _)) = listener.accept() {
                let mut line = String::new();
                if BufReader::new(conn).read_line(&mut line).is_ok() && !line.is_empty() {
                    kept.lock()
                        .unwrap()
                        .push(serde_json::from_str(&line).unwrap());
                }
            }
        });
        Daemon { socket, lines }
    }

    /// The asks that the daemon has now, after it got at least `count`.
    fn asks(&self, count: usize) -> Vec<serde_json::Value> {
        let end = Instant::now() + Duration::from_secs(10);
        while self.lines.lock().unwrap().len() < count && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.lines.lock().unwrap().clone()
    }
}

/// One container: the `gh` of the wrapper, the real `gh` that it runs, and
/// the fake daemon.
struct Container {
    env: Isolated,
    dir: tempfile::TempDir,
    daemon: Daemon,
}

impl Container {
    /// The real `gh` runs `body`: a shell script.
    fn new(body: &str) -> Container {
        let dir = tempfile::tempdir().unwrap();
        let env = Isolated::new();
        std::os::unix::fs::symlink(env.riff_path(), dir.path().join("gh")).unwrap();
        isolated::write_program(dir.path().join("real-gh"), format!("#!/bin/sh\n{body}\n"));
        let daemon = Daemon::start(dir.path());
        Container { env, dir, daemon }
    }

    /// `gh ARGS` with `stdin`, with the variables `vars`.
    fn gh(&self, args: &[&str], stdin: &str, vars: &[(&str, &str)]) -> Output {
        let mut cmd: Command = self.env.command(self.dir.path().join("gh"));
        cmd.args(args)
            .env("RIFF_SOCKET", &self.daemon.socket)
            .env("RIFF_GH_REAL", self.dir.path().join("real-gh"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in vars {
            cmd.env(name, value);
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The `calls` of an ask of a session: `(caller, kind, points)`.
fn calls(ask: &serde_json::Value) -> Vec<(String, String, u64)> {
    ask["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["caller"].as_str().unwrap().to_owned(),
                c["kind"].as_str().unwrap().to_owned(),
                c["points"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// The wrapper sends the call to the daemon on the socket of the session
/// and then runs the real `gh` in its place: the arguments, the stdin,
/// the stdout, the stderr and the exit code are those of the real `gh`.
#[test]
fn the_gh_of_a_container_counts_the_call_and_is_the_real_gh_to_its_caller() {
    let container = Container::new("echo \"args: $*\"; cat; echo oops >&2; exit 3");
    let out = container.gh(
        &["pr", "view", "40", "--json", "state"],
        "from stdin\n",
        &[],
    );
    assert_eq!(out.status.code(), Some(3), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "args: pr view 40 --json state\nfrom stdin\n"
    );
    assert_eq!(text(&out.stderr), "oops\n");

    let asks = container.daemon.asks(1);
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert_eq!(asks[0]["ask"], "gh_calls");
    assert_eq!(asks[0]["protocol"], riff::daemon::PROTOCOL);
    assert_eq!(
        calls(&asks[0]),
        [("session".to_owned(), "pr view".to_owned(), 1)]
    );
}

/// A call that uses no limit is not counted, and a call that a command of
/// riff counted already (`RIFF_GH_COUNTED`) is not counted twice. A call of
/// a kind with points is counted with the points of its table.
#[test]
fn the_wrapper_counts_nothing_for_a_call_with_no_limit_or_one_that_riff_counted() {
    let container = Container::new("echo ok");
    for args in [&["auth", "status"][..], &["--version"], &["config", "list"]] {
        let out = container.gh(args, "", &[]);
        assert_eq!(text(&out.stdout), "ok\n", "{args:?}");
    }
    let out = container.gh(&["pr", "view", "40"], "", &[("RIFF_GH_COUNTED", "1")]);
    assert_eq!(text(&out.stdout), "ok\n");
    // The last call is counted. The daemon takes the connections in order,
    // so the earlier calls would be in the asks before it.
    container.gh(
        &["pr", "list", "--json", "number,statusCheckRollup"],
        "",
        &[],
    );
    container.gh(&["api", "repos/o/r"], "", &[]);
    let asks = container.daemon.asks(2);
    let all: Vec<_> = asks.iter().flat_map(calls).collect();
    assert_eq!(
        all,
        [
            ("session".to_owned(), "pr list".to_owned(), 2),
            ("session".to_owned(), "api repo".to_owned(), 0),
        ]
    );
}

/// With no daemon on the socket the call runs, and fails in no other way:
/// a count never fails or delays a call.
#[test]
fn the_wrapper_runs_the_call_when_the_daemon_does_not_answer() {
    let container = Container::new("echo ok");
    std::fs::remove_file(&container.daemon.socket).unwrap();
    let start = Instant::now();
    let out = container.gh(&["issue", "list"], "", &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok\n");
    assert!(start.elapsed() < Duration::from_secs(5));
    // No socket variable at all: the folder of the daemon of the machine.
    let mut cmd: Command = container.env.command(container.dir.path().join("gh"));
    let out = cmd
        .args(["issue", "list"])
        .env_remove("RIFF_SOCKET")
        .env("RIFF_GH_REAL", container.dir.path().join("real-gh"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let folder = container.env.riff_home().join("daemon/gh-calls");
    let files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].contains(r#""caller":"session""#), "{files:?}");
    assert!(files[0].contains(r#""kind":"issue list""#), "{files:?}");
}

/// When the real `gh` is missing the wrapper says so, with the status
/// that a shell gives for a program that is not there.
#[test]
fn the_wrapper_fails_with_127_when_the_real_gh_is_missing() {
    let container = Container::new("echo ok");
    std::fs::remove_file(container.dir.path().join("real-gh")).unwrap();
    let out = container.gh(&["issue", "list"], "", &[]);
    assert_eq!(out.status.code(), Some(127));
    assert!(
        text(&out.stderr).contains("gh: cannot run"),
        "{}",
        text(&out.stderr)
    );
}

/// A name other than `gh` is the plain `riff`: it counts nothing and runs
/// no `gh`.
#[test]
fn riff_under_another_name_is_not_the_wrapper() {
    let container = Container::new("echo ok");
    let other = container.dir.path().join("not-gh");
    std::os::unix::fs::symlink(container.env.riff_path(), &other).unwrap();
    let out = container
        .env
        .command(&other)
        .arg("--version")
        .output()
        .unwrap();
    assert!(
        text(&out.stdout).starts_with("riff "),
        "{}",
        text(&out.stdout)
    );
    assert!(container.daemon.lines.lock().unwrap().is_empty());
}
