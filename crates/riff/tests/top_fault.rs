//! `riff top` stays open when a look fails because riff cannot reach
//! the server (01M3Z8FXE2DY34ZP75WJE1S8HR). It keeps the last table,
//! shows one line with the fault and the time of the last good look,
//! and shows a new table when the server is back.
//!
//! Each test puts a [`Gate`] between `riff top` and a real
//! `riff-server`. The gate is the network: a test closes it, breaks
//! each connection, or lets another server answer.

use isolated::Isolated;
use std::io::Read;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff_core::name::SessionUri;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// The start of the line of a look that failed.
const FAULT: &str = "riff: the last good look was at ";

/// What the gate does with a new connection.
#[derive(Clone, Copy)]
enum Mode {
    /// It passes each byte to and from the server.
    Pass,
    /// It closes the connection with no reply: a connection that fails
    /// in the middle of a call.
    Break,
    /// It replies as a server that is not `riff-server`: 404, with no
    /// build.
    Other,
}

/// A TCP port in front of a `riff-server`.
struct Gate {
    addr: SocketAddr,
    server: SocketAddr,
    accept: Option<JoinHandle<()>>,
    links: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Gate {
    /// A real riff-server with no sign-in, and an open gate to it.
    async fn start() -> Gate {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, riff_server::router()).await.unwrap();
        });
        let mut gate = Gate {
            addr: "127.0.0.1:0".parse().unwrap(),
            server,
            accept: None,
            links: Arc::default(),
        };
        gate.open(Mode::Pass).await;
        gate
    }

    /// The URL that `riff top` uses.
    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// A client that talks to the server, not through the gate.
    fn direct(&self) -> Api {
        Api::new(&format!("http://{}", self.server))
    }

    /// Closes the port, and ends each open connection: each new connect
    /// is refused.
    async fn close(&mut self) {
        if let Some(accept) = self.accept.take() {
            accept.abort();
            let _ = accept.await;
        }
        let links: Vec<_> = self.links.lock().unwrap().drain(..).collect();
        for link in links {
            link.abort();
            let _ = link.await;
        }
    }

    /// Opens the port again, on the same address, with `mode` for each
    /// new connection.
    async fn open(&mut self, mode: Mode) {
        self.close().await;
        let listener = TcpListener::bind(self.addr).await.unwrap();
        self.addr = listener.local_addr().unwrap();
        let (server, links) = (self.server, Arc::clone(&self.links));
        self.accept = Some(tokio::spawn(async move {
            loop {
                let (mut client, _) = listener.accept().await.unwrap();
                let link = tokio::spawn(async move {
                    match mode {
                        Mode::Pass => {
                            let mut up = TcpStream::connect(server).await.unwrap();
                            let _ = tokio::io::copy_bidirectional(&mut client, &mut up).await;
                        }
                        Mode::Break => {
                            // The request comes, and no reply goes back.
                            let _ = client.read(&mut [0; 1024]).await;
                        }
                        Mode::Other => {
                            let _ = client.read(&mut [0; 4096]).await;
                            let reply = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\
                                         connection: close\r\n\r\n";
                            let _ = client.write_all(reply.as_bytes()).await;
                        }
                    }
                });
                links.lock().unwrap().push(link);
            }
        }));
    }
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

/// A dir for `PATH` with `git`, and no `gh`.
fn bin() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
    let tmux = dir.path().join("tmux");
    std::fs::write(&tmux, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

/// `riff top ARGS` in `dir` through a pipe, as the session `a1` of
/// mike, with `path` as `PATH`.
fn top(server: &str, dir: &Path, path: &Path, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.arg("top")
        .args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_HOME", dir)
        .env("RIFF_USER", "mike")
        .env("RIFF_SESSION", "a1")
        .env("PATH", path)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// A `riff top` that runs until the test ends, and what it printed.
struct Live {
    child: Child,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

impl Live {
    /// Starts `riff top` at `server`.
    fn start(server: &str) -> Live {
        let dirs = (repo(), bin());
        let mut child = top(server, dirs.0.path(), dirs.1.path(), &[])
            .spawn()
            .unwrap();
        let stdout = follow(child.stdout.take().unwrap());
        let stderr = follow(child.stderr.take().unwrap());
        Live {
            child,
            stdout,
            stderr,
            _dirs: dirs,
        }
    }

    /// Each table that `riff top` printed until now. In a pipe, it
    /// prints one table after the other.
    fn views(&self) -> Vec<String> {
        let text = self.stdout.lock().unwrap().clone();
        let mut views: Vec<String> = Vec::new();
        let mut after_fault = false;
        for line in text.lines() {
            let fault = line.starts_with(FAULT);
            if fault || (line.starts_with("riff   ") && !after_fault) {
                views.push(String::new());
            }
            after_fault = fault;
            if let Some(view) = views.last_mut() {
                view.push_str(line);
                view.push('\n');
            }
        }
        views
    }

    /// Waits until a table from the index `from` on passes `found`, and
    /// gives its index and the table. It fails after 40 seconds.
    async fn view(&mut self, from: usize, what: &str, found: impl Fn(&str) -> bool) -> (usize, String) {
        let end = Instant::now() + Duration::from_secs(40);
        loop {
            let views = self.views();
            if let Some(hit) = views.iter().enumerate().skip(from).find(|(_, v)| found(v)) {
                return (hit.0, hit.1.clone());
            }
            let ended = self.child.try_wait().unwrap();
            assert!(
                ended.is_none() && Instant::now() < end,
                "no table with {what}; riff top ended: {ended:?}\nstdout:\n{}\nstderr:\n{}",
                views.join("--\n"),
                self.stderr.lock().unwrap()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Waits until `riff top` ends by itself, and gives true when its
    /// status is 0. It fails after 40 seconds.
    async fn ended(&mut self) -> bool {
        let end = Instant::now() + Duration::from_secs(40);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            assert!(Instant::now() < end, "riff top did not end");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads `from` to its end in a thread, into the text that it returns.
fn follow(mut from: impl Read + Send + 'static) -> Arc<Mutex<String>> {
    let text = Arc::new(Mutex::new(String::new()));
    let into = Arc::clone(&text);
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n @ 1..) = from.read(&mut buf) {
            into.lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    });
    text
}

/// Registers the session `id` of mike on thelio at the server.
async fn session(api: &Api, id: &str) {
    let uri: SessionUri = format!("riff://mike@thelio/como-technologies/riff?session={id}")
        .parse()
        .unwrap();
    api.register(&uri).await.unwrap();
}

/// True when `view` is a full table with the row of the session `id`.
fn has(view: &str, id: &str) -> bool {
    let table = view.starts_with("riff   ") || view.contains("\nriff   ");
    table && view.contains(&format!("─ {id}  "))
}

/// The steps of each test with a short fault: a good table, the fault
/// with `away`, the last table with the line, then the server is back
/// and the table is new, with no line.
async fn stays_open(away: Option<Mode>) {
    let mut gate = Gate::start().await;
    session(&gate.direct(), "b2").await;
    let mut live = Live::start(&gate.url());
    let good = |v: &str| !v.starts_with(FAULT) && has(v, "b2");
    live.view(0, "the session b2", good).await;

    // The server goes away.
    match away {
        Some(mode) => gate.open(mode).await,
        None => gate.close().await,
    }
    let kept = |v: &str| v.starts_with(FAULT) && has(v, "b2");
    let (at, view) = live.view(0, "the line and the last table", kept).await;
    let line = view.lines().next().unwrap();
    let time = line.strip_prefix(FAULT).unwrap();
    let (time, fault) = time.split_once(". riff tries again: ").expect(line);
    assert_eq!(time.len(), "21:35:07".len(), "{line}");
    assert!(
        fault.contains(&format!("riff-server at {}", gate.url())),
        "{line}"
    );
    assert!(!view.contains("─ d4  "), "{view}");
    assert!(
        view.lines().filter(|l| l.starts_with("riff: ")).count() == 1,
        "one line: {view}"
    );

    // A new session comes while the server is away, then the server is
    // back.
    session(&gate.direct(), "d4").await;
    gate.open(Mode::Pass).await;
    let new = |v: &str| !v.starts_with(FAULT) && has(v, "b2") && has(v, "d4");
    live.view(at + 1, "the new session d4 and no line", new)
        .await;
    assert!(live.child.try_wait().unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn top_stays_open_while_the_server_is_away() {
    stays_open(None).await;
}

/// The second case of the fault: the address of the machine changes, so
/// each open connection fails in the middle. A new connection works.
#[tokio::test(flavor = "multi_thread")]
async fn top_stays_open_when_each_connection_fails_in_the_middle() {
    stays_open(Some(Mode::Break)).await;
}

/// A new try cannot repair a server of another kind: `riff top` ends
/// with the text of the fault.
#[tokio::test(flavor = "multi_thread")]
async fn top_ends_on_a_fault_that_a_new_try_cannot_repair() {
    let mut gate = Gate::start().await;
    session(&gate.direct(), "b2").await;
    let mut live = Live::start(&gate.url());
    live.view(0, "the session b2", |v| has(v, "b2")).await;

    gate.open(Mode::Other).await;
    assert!(!live.ended().await, "the status is not 0");
    let stderr = live.stderr.lock().unwrap().clone();
    assert!(stderr.contains("status 404 from"), "{stderr}");
    let views = live.views();
    assert!(!views.iter().any(|v| v.starts_with(FAULT)), "{views:?}");
}

/// `riff top --once` ends with the error and a status that is not 0.
/// A `riff top` whose first look fails ends in the same way.
#[test]
fn top_once_and_a_first_look_end_with_the_error() {
    let (repo, bin) = (repo(), bin());
    for args in [&["--once"][..], &[]] {
        // Nothing listens on port 9.
        let out = top("http://127.0.0.1:9", repo.path(), bin.path(), args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {stderr}");
        assert!(
            stderr.contains("cannot reach riff-server at http://127.0.0.1:9"),
            "{args:?}: {stderr}"
        );
        assert!(out.stdout.is_empty(), "{args:?}");
    }
}

/// `riff top --once` ends with the error also at a server that gave a
/// reply before the fault: it never keeps a table.
#[tokio::test(flavor = "multi_thread")]
async fn top_once_ends_with_the_error_when_a_connection_fails() {
    let mut gate = Gate::start().await;
    gate.open(Mode::Break).await;
    let (repo, bin) = (repo(), bin());
    let mut cmd = top(&gate.url(), repo.path(), bin.path(), &["--once"]);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("cannot reach riff-server at"), "{stderr}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains(FAULT));
}
