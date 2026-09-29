//! Workers on another machine of the user (01M3N7AK8TVYV8S0WR3RP0TN8X
//! to 01M3N7AKFPX3ZGQARSG2V64GBD). Two machines, `a` and `b`, each with
//! a fake `tmux` on `PATH` that writes each call to a log and keeps the
//! worker panes in a file. The lead runs on `a`; `riff workers host`
//! runs on `b`. A line in the file `slow` of a fake `tmux` is a pattern:
//! a call that matches it sleeps for 30 seconds. As in tmux, a pane
//! whose program does not exist ends at once, so a later call on it
//! fails, and the window of the workers closes with its last pane.

use isolated::Isolated;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::{RiffState, Status};

/// `list-panes -a` lists the worker panes with their session marks, from
/// the file `workers`. `kill-pane` removes a pane from it, and the
/// window when it was the last pane. A new pane whose program does not
/// exist goes to the file `dead`: `set-option` on it fails.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
if [ -f "$dir/slow" ]; then
  case "$*" in $(cat "$dir/slow")) sleep 30 ;; esac
fi
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
case "$1" in
  new-window|split-window)
    for last; do :; done
    program=$(eval "set -- $last"; printf '%s' "$1")
    [ -e "$program" ] || echo "%$n" >> "$dir/dead" ;;
  set-option)
    if [ "$2" = "-p" ] && grep -qx -- "$4" "$dir/dead" 2>/dev/null; then
      echo "no such pane: $4" >&2
      exit 1
    fi ;;
esac
case "$1" in
  list-panes)
    if [ "$2" = "-a" ]; then cat "$dir/workers" 2>/dev/null; else cat "$dir/panes" 2>/dev/null; fi ;;
  list-windows) cat "$dir/windows" 2>/dev/null ;;
  display-message) echo "@0" ;;
  new-window) echo "@7 %$n" ;;
  split-window) echo "%$n" ;;
  set-option)
    case "$2 $5" in
      "-p @riff-session") echo "$4 $6" >> "$dir/workers" ;;
      "-w @riff") echo "$4 $6" >> "$dir/windows" ;;
    esac ;;
  kill-pane)
    grep -v "^$3 " "$dir/workers" > "$dir/workers.new"
    mv "$dir/workers.new" "$dir/workers"
    [ -s "$dir/workers" ] || rm -f "$dir/windows" ;;
esac
exit 0
"#;

const WAIT: Duration = Duration::from_secs(20);

/// One machine: a fake `tmux`, its own riff home, and its host name.
struct Machine {
    fake: tempfile::TempDir,
    home: tempfile::TempDir,
    host: &'static str,
    server: String,
}

impl Machine {
    fn new(host: &'static str, server: &str) -> Self {
        let fake = tempfile::tempdir().unwrap();
        let tmux = fake.path().join("tmux");
        std::fs::write(&tmux, FAKE_TMUX).unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        Machine {
            fake,
            home: tempfile::tempdir().unwrap(),
            host,
            server: server.into(),
        }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.fake.path().join("log")).unwrap_or_default()
    }

    /// The worker panes of the fake tmux: `PANE SESSION` lines.
    fn workers(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(self.fake.path().join("workers"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(p, s)| (p.to_owned(), s.to_owned()))
            .collect()
    }

    /// `riff workers ARGS` of mike in `dir`, in a tmux pane. `session`
    /// is the agent session, or `None` for a plain terminal.
    fn riff(&self, dir: &Path, args: &[&str], session: Option<&str>) -> Command {
        self.riff_at(&Isolated::shared().riff_path(), dir, args, session)
    }

    /// [`Machine::riff`] with the `riff` binary at `binary`.
    fn riff_at(&self, binary: &Path, dir: &Path, args: &[&str], session: Option<&str>) -> Command {
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().command(binary);
        cmd.arg("workers")
            .args(args)
            .current_dir(dir)
            .env("PATH", path)
            .env("RIFF_HOME", self.home.path())
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", self.host)
            .env("TMUX", "/tmp/tmux-1000/default,1,0")
            .env("TMUX_PANE", "%0")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER");
        match session {
            Some(id) => cmd.env("RIFF_SESSION", id),
            None => cmd.env_remove("RIFF_SESSION"),
        };
        cmd
    }

    fn limit(&self, limit: u16) {
        let out = self
            .riff(Path::new("/"), &["limit", &limit.to_string()], None)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }

    /// Starts `riff workers host` in `dir`. It stops when the value
    /// drops.
    fn host(&self, dir: &Path) -> Running {
        self.host_with(self.riff(dir, &["host", "--claude", "true"], None))
    }

    /// Starts `cmd`, a `riff workers host`. Its input is the file
    /// `host.in` with a line of keys; its output goes to `host.out` and
    /// `host.err` of the fake dir.
    fn host_with(&self, mut cmd: Command) -> Running {
        let file = |name: &str| self.fake.path().join(name);
        std::fs::write(file("host.in"), "keys of the person\n").unwrap();
        let child = cmd
            .stdin(std::fs::File::open(file("host.in")).unwrap())
            .stdout(std::fs::File::create(file("host.out")).unwrap())
            .stderr(std::fs::File::create(file("host.err")).unwrap())
            .spawn()
            .unwrap();
        Running(child)
    }

    /// The output of the host so far: stdout, then stderr.
    fn host_output(&self) -> String {
        let read =
            |name: &str| std::fs::read_to_string(self.fake.path().join(name)).unwrap_or_default();
        read("host.out") + &read("host.err")
    }

    /// Makes each later call of the fake `tmux` that matches `pattern`
    /// sleep for 30 seconds.
    fn slow(&self, pattern: &str) {
        std::fs::write(self.fake.path().join("slow"), pattern).unwrap();
    }
}

/// A process that is killed on drop.
struct Running(Child);

impl Running {
    /// Sends `signal` to the process, and returns the time until it ends.
    fn stop(&mut self, signal: &str) -> Duration {
        let start = Instant::now();
        let kill = Command::new("kill")
            .args([&format!("-{signal}"), &self.0.id().to_string()])
            .status()
            .unwrap();
        assert!(kill.success());
        loop {
            if self.0.try_wait().unwrap().is_some() {
                return start.elapsed();
            }
            assert!(start.elapsed() < WAIT, "the host did not stop on {signal}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A repository with a commit, so that it has a main worktree.
fn repository(root: &Path) -> PathBuf {
    let main = root.join("riff");
    std::fs::create_dir(&main).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "x"]);
    std::fs::canonicalize(&main).unwrap()
}

/// The session `id` of `user` on `host`, at the place of `dir`.
fn session(dir: &Path, user: &str, host: &str, id: &str) -> SessionUri {
    let place = identity::place_in(dir, host).unwrap();
    SessionUri::new(Who::new(user, Some(id)).unwrap(), place)
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Waits until `check` gives `Some`.
async fn until<T, F, Fut>(what: &str, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let start = Instant::now();
    loop {
        if let Some(value) = check().await {
            return value;
        }
        assert!(start.elapsed() < WAIT, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The session ID of the live workers host on `host`.
async fn host_session(api: &Api, me: &SessionUri, host: &str) -> String {
    until("the host in riff who", || async {
        let who = api.who(me, false).await.ok()?;
        who.iter()
            .find(|s| {
                s.live
                    && s.uri.place().host() == host
                    && s.status
                        .as_ref()
                        .is_some_and(|st| st.status.step.starts_with("workers host"))
            })
            .and_then(|s| s.uri.who().session().map(str::to_owned))
    })
    .await
}

/// The unread text of `me`, when it contains `needle`.
async fn reads(api: &Api, me: &SessionUri, needle: &str) -> String {
    let start = Instant::now();
    let mut all = String::new();
    loop {
        let inbox = api.inbox(me, None, false).await.unwrap();
        all.push_str(&riff::text::inbox(&inbox, me));
        if all.contains(needle) {
            return all;
        }
        assert!(start.elapsed() < WAIT, "timed out: {needle}\n{all}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The lead, the host on `b`, and a riff that runs.
struct Riff {
    api: Api,
    a: Machine,
    b: Machine,
    main: PathBuf,
    lead: SessionUri,
    _root: tempfile::TempDir,
    host: Running,
    /// The session ID of the host.
    host_id: String,
}

async fn riff() -> Riff {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let a = Machine::new("a", api.base());
    let b = Machine::new("b", api.base());
    b.limit(3);
    let host = b.host(&main);
    let host_id = host_session(&api, &lead, "b").await;
    Riff {
        api,
        a,
        b,
        main,
        lead,
        _root: root,
        host,
        host_id,
    }
}

/// The lead on `a` starts 2 workers on `b` through `riff workers host`;
/// `riff workers` on `a` lists them with host `b`
/// (01M3N7AKB3KXS2XYK0309C4M18, 01M3N7AKFPX3ZGQARSG2V64GBD).
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_starts_workers_on_another_host() {
    let r = riff().await;
    let out =
        r.a.riff(&r.main, &["start", "2", "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains("Asked the workers host on b: workers start 2."),
        "{out:?}"
    );
    let read = reads(&r.api, &r.lead, "b: started 2 workers").await;
    assert!(
        read.contains("mike@b:riff") && read.contains("note: b: started"),
        "{read}"
    );

    let workers = r.b.workers();
    assert_eq!(workers.len(), 2, "{}", r.b.log());
    assert!(r.a.workers().is_empty(), "{}", r.a.log());
    for (_, id) in &workers {
        assert!(read.contains(id.as_str()), "{read}");
        let me = session(&r.main, "mike", "b", id);
        r.api.register(&me).await.unwrap();
        let status = Status {
            step: "idle: waits for work".into(),
            blocked: None,
        };
        r.api.status(&me, &status).await.unwrap();
    }

    let listed = until("riff workers lists host b", || async {
        let out = r.a.riff(&r.main, &[], Some("l1")).output().unwrap();
        let out = stdout(&out);
        out.contains("Host b: limit 3, 2 workers run.")
            .then_some(out)
    })
    .await;
    assert!(
        listed.contains("\nNo worker runs on this machine.\n"),
        "{listed}"
    );
    // Each machine shows its numbers and its score
    // (01M3Q5QE4SQ8VYN2PSF42KB3QJ).
    let first = listed.lines().next().unwrap();
    assert!(first.starts_with("This machine: limit 0. cpu "), "{listed}");
    assert!(first.contains(", score "), "{listed}");
    let host = listed.lines().find(|l| l.starts_with("Host b:")).unwrap();
    assert!(
        host.contains(". cpu ") && host.contains(", score "),
        "{listed}"
    );
    for (pane, id) in &workers {
        assert!(
            listed.contains(&format!("{pane}  {}  {id}", &id[..8])),
            "{listed}"
        );
    }
    assert!(listed.contains("idle: waits for work"), "{listed}");
}

/// A host refuses a start request that is not from the lead of its
/// user: from another session of the user, and from the lead of another
/// user (01M3N7AKDE7DEA6NXS9ZMECRMH). The doc test of `host::judge`
/// shows the refusal of a request that is not verified.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_refuses_a_request_that_is_not_from_the_lead() {
    let r = riff().await;
    let host = host_session(&r.api, &r.lead, "b").await;

    let worker = session(&r.main, "mike", "a", "w1");
    r.api.register(&worker).await.unwrap();
    r.api.tell(&worker, &host, "workers start 1").await.unwrap();
    let read = reads(&r.api, &worker, "refused").await;
    assert!(read.contains("is not from the lead of mike in"), "{read}");

    let brett = session(&r.main, "brett", "c", "k1");
    r.api.register(&brett).await.unwrap();
    assert!(
        r.api
            .who(&brett, false)
            .await
            .unwrap()
            .iter()
            .any(|s| s.uri.who() == brett.who() && s.uri.lead()),
        "brett's session is the lead of brett"
    );
    r.api.tell(&brett, &host, "workers start 1").await.unwrap();
    let read = reads(&r.api, &brett, "refused").await;
    assert!(read.contains("is not from the lead of mike"), "{read}");

    assert!(r.b.workers().is_empty(), "{}", r.b.log());
}

/// `riff workers stop --host b` stops the workers of `b` only
/// (01M3N7AKB3KXS2XYK0309C4M18).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_on_a_host_stops_only_its_workers() {
    let r = riff().await;
    std::fs::write(r.a.fake.path().join("workers"), "%9 x1\n").unwrap();
    let out =
        r.a.riff(&r.main, &["start", "2", "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    reads(&r.api, &r.lead, "b: started 2 workers").await;
    assert_eq!(r.b.workers().len(), 2);

    let out =
        r.a.riff(&r.main, &["stop", "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains("Asked the workers host on b: workers stop."),
        "{out:?}"
    );
    reads(&r.api, &r.lead, "b: Stopped 2 workers.").await;
    assert!(r.b.workers().is_empty(), "{}", r.b.log());
    assert_eq!(r.a.workers(), [("%9".to_owned(), "x1".to_owned())]);
    assert!(!r.a.log().contains("kill-pane"), "{}", r.a.log());
}

/// `riff workers stop PANE --host b` stops that one worker on `b`; the
/// other workers there go on. The start of a session ID works as PANE
/// too (01M3Q5A0Z5DK0YV1MWTM4AQD5Z).
#[tokio::test(flavor = "multi_thread")]
async fn workers_stop_pane_on_a_host_stops_only_that_worker() {
    let r = riff().await;
    let out =
        r.a.riff(&r.main, &["start", "3", "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    reads(&r.api, &r.lead, "b: started 3 workers").await;
    let workers = r.b.workers();
    assert_eq!(workers.len(), 3);

    let (pane, _) = &workers[0];
    let out =
        r.a.riff(&r.main, &["stop", pane, "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        stdout(&out).contains(&format!(
            "Asked the workers host on b: workers stop {pane}."
        )),
        "{out:?}"
    );
    reads(&r.api, &r.lead, "b: Stopped 1 worker.").await;
    assert_eq!(r.b.workers(), workers[1..]);

    let (_, session) = &workers[1];
    let out =
        r.a.riff(&r.main, &["stop", &session[..8], "--host", "b"], Some("l1"))
            .output()
            .unwrap();
    assert!(out.status.success(), "{out:?}");
    reads(&r.api, &r.lead, "b: Stopped 1 worker.").await;
    assert_eq!(r.b.workers(), workers[2..]);
}

/// `riff workers start --host` names `riff workers host` when no host
/// runs there.
#[tokio::test(flavor = "multi_thread")]
async fn a_start_on_a_host_with_no_workers_host_fails() {
    let r = riff().await;
    let out =
        r.a.riff(&r.main, &["start", "1", "--host", "z"], Some("l1"))
            .output()
            .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no workers host of your user runs on z"),
        "{out:?}"
    );
}

/// Puts a copy of `from` at `to` as `cargo install` does: a new file
/// beside it, then a rename. A child process copies, so that no fork of
/// a parallel test holds a write fd of the file.
fn install(from: &Path, to: &Path) {
    let stage = to.with_extension("stage");
    let out = Command::new("cp").arg(from).arg(&stage).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    std::fs::rename(&stage, to).unwrap();
}

/// The inode of the binary that the process `pid` runs.
fn runs(pid: u32) -> u64 {
    std::fs::metadata(format!("/proc/{pid}/exe")).unwrap().ino()
}

/// A riff that runs, the lead on `a`, and machine `b` with limit 3 and
/// its own copy of `riff` at `binary`.
struct Copy {
    api: Api,
    b: Machine,
    main: PathBuf,
    lead: SessionUri,
    binary: PathBuf,
    _root: tempfile::TempDir,
}

async fn copy() -> Copy {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let b = Machine::new("b", api.base());
    b.limit(3);
    let binary = root.path().join("bin/riff");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    install(&Isolated::shared().riff_path(), &binary);
    Copy {
        api,
        b,
        main,
        lead,
        binary,
        _root: root,
    }
}

impl Copy {
    /// Starts `riff workers host` on `b` from the copy of `riff`.
    fn host(&self) -> Running {
        let args = ["host", "--claude", "true"];
        self.b
            .host_with(self.b.riff_at(&self.binary, &self.main, &args, None))
    }
}

/// A running host sees a new `riff` installed as `cargo install` does
/// it, runs it in its place with the same session, and answers the next
/// start request of the lead (01M3Q55KJ8BKMPE9RADB63X8SP).
#[tokio::test(flavor = "multi_thread")]
async fn a_host_runs_a_new_binary_and_answers_the_next_start() {
    let c = copy().await;
    let host = c.host();
    let id = host_session(&c.api, &c.lead, "b").await;
    let pid = host.0.id();

    install(&Isolated::shared().riff_path(), &c.binary);
    let new = std::fs::metadata(&c.binary).unwrap().ino();
    until("the host runs the new binary", || async {
        (runs(pid) == new).then_some(())
    })
    .await;
    assert!(
        c.b.host_output().contains("a new riff is on disk"),
        "{}",
        c.b.host_output()
    );

    c.api.tell(&c.lead, &id, "workers start 1").await.unwrap();
    let read = reads(&c.api, &c.lead, "b: started 1 worker").await;
    assert!(read.contains("mike@b"), "{read}");
    assert_eq!(c.b.workers().len(), 1, "{}", c.b.log());
    assert_eq!(host_session(&c.api, &c.lead, "b").await, id);
    assert!(host.0.id() == pid && runs(pid) == new);
}

/// Stop all workers of a host, so that the window of the workers
/// closes. A new `riff` is on disk, but the host still runs the old one.
/// Start 1: it starts, in a new window, and its pane runs the `riff` on
/// disk (01M3Q55KMQSSJVQEN86XFB8PSG).
#[tokio::test(flavor = "multi_thread")]
async fn a_start_after_a_stop_of_all_workers_starts() {
    let c = copy().await;
    let host = c.host();
    let id = host_session(&c.api, &c.lead, "b").await;
    c.api.tell(&c.lead, &id, "workers start 2").await.unwrap();
    reads(&c.api, &c.lead, "b: started 2 workers").await;
    c.api.tell(&c.lead, &id, "workers stop").await.unwrap();
    reads(&c.api, &c.lead, "b: Stopped 2 workers.").await;
    assert!(c.b.workers().is_empty(), "{}", c.b.log());
    assert!(
        !c.b.fake.path().join("windows").exists(),
        "the window closed"
    );

    // A new binary replaces the old one, and changes at each poll, so the
    // host does not run it yet: its own binary is `riff (deleted)` now.
    install(&Isolated::shared().riff_path(), &c.binary);
    let old = runs(host.0.id());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let touch = {
        let (stop, binary) = (stop.clone(), c.binary.clone());
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = Command::new("touch").arg(&binary).status();
                std::thread::sleep(Duration::from_millis(200));
            }
        })
    };

    c.api.tell(&c.lead, &id, "workers start 1").await.unwrap();
    let read = reads(&c.api, &c.lead, "b: ").await;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    touch.join().unwrap();
    assert_eq!(
        runs(host.0.id()),
        old,
        "the host ran the new binary too early"
    );
    assert!(
        read.contains("b: started 1 worker"),
        "{read}\n{}",
        c.b.log()
    );
    assert_eq!(c.b.workers().len(), 1, "{}", c.b.log());
    let log = c.b.log();
    assert_eq!(log.matches("new-window").count(), 2, "{log}");
    assert!(!log.contains("(deleted)"), "{log}");
}

/// The book has the how-to with the real commands, each in `--help`,
/// and the skill tells the lead to start workers on each host first
/// (01M3N7AKHXGYQ58G61BEHS89WG).
#[test]
fn the_book_and_the_skill_say_how_to_use_a_host() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let book = std::fs::read_to_string(root.join("../../docs/src/how-it-works.md")).unwrap();
    let how = &book[book.find("### Offer workers from another machine").unwrap()..];
    let how = &how[..how[4..].find("\n### ").map_or(how.len(), |n| n + 4)];
    for command in [
        "```sh\nriff workers limit 2\nriff workers host\n```",
        "```sh\nriff workers start 2 --host pangolin\nriff workers stop --host pangolin\n```",
        "```mermaid",
    ] {
        assert!(how.contains(command), "the how-to has no {command:?}");
    }
    for (args, flag) in [
        (&["workers", "--help"][..], "host "),
        (&["workers", "start", "--help"][..], "--host <HOST>"),
        (&["workers", "stop", "--help"][..], "--host <HOST>"),
        (&["workers", "host", "--help"][..], "--claude <CLAUDE>"),
    ] {
        let help = Isolated::shared().riff().args(args).output().unwrap();
        let help = String::from_utf8_lossy(&help.stdout);
        assert!(help.contains(flag), "{args:?}: {help}");
    }
    let skill =
        std::fs::read_to_string(root.join("claude-plugin/riff/skills/riff/SKILL.md")).unwrap();
    assert!(skill.contains("riff workers start N --host HOST"), "skill");
    assert!(
        skill.contains("on each host, then\n   on your own machine last"),
        "skill"
    );
}

/// `riff workers host` needs tmux and a limit (01M3N7AK8TVYV8S0WR3RP0TN8X).
#[tokio::test(flavor = "multi_thread")]
async fn a_host_needs_tmux_and_a_limit() {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let b = Machine::new("b", api.base());
    let out = b.riff(&main, &["host"], None).output().unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("riff workers limit 2"),
        "{out:?}"
    );
    b.limit(1);
    let out = b
        .riff(&main, &["host"], None)
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("needs tmux"),
        "{out:?}"
    );
}

/// Sends `signal` to the host of `r`: it ends in under 2 seconds, and
/// its session leaves `riff who` (01M3NBV405PVYHKTMQ5VN87FYN).
async fn stops_on(r: &mut Riff, signal: &str, state: &str) {
    let took = r.host.stop(signal);
    assert!(
        took < Duration::from_secs(2),
        "{state}: {signal} took {took:?}"
    );
    let who = r.api.who(&r.lead, false).await.unwrap();
    assert!(
        !who.iter()
            .any(|s| s.uri.who().session() == Some(r.host_id.as_str())),
        "{state}: the session of the host is still in riff who"
    );
    assert!(
        r.b.host_output().contains("riff workers host stopped."),
        "{state}: {}",
        r.b.host_output()
    );
}

/// Waits until the fake `tmux` of `m` logs a call that contains `call`,
/// after its first `before` calls.
async fn calls(m: &Machine, call: &str, before: usize) {
    until(call, || async {
        m.log()
            .lines()
            .skip(before)
            .any(|l| l.contains(call))
            .then_some(())
    })
    .await;
}

/// Ctrl-C stops a host that waits for a wake, that answers a request (a
/// slow `tmux new-window`), and that sets its status (a slow `tmux
/// list-panes -a`) (01M3NBV405PVYHKTMQ5VN87FYN).
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_stops_the_host_in_each_state() {
    let mut r = riff().await;
    stops_on(&mut r, "INT", "waiting for a wake").await;

    let mut r = riff().await;
    r.b.slow("*-window*");
    let before = r.b.log().lines().count();
    r.api
        .tell(&r.lead, &r.host_id, "workers start 1")
        .await
        .unwrap();
    calls(&r.b, "-window", before).await;
    stops_on(&mut r, "INT", "answering a request").await;

    let mut r = riff().await;
    r.b.slow("list-panes -a*");
    let before = r.b.log().lines().count();
    r.api.tell(&r.lead, &r.host_id, "hello").await.unwrap();
    calls(&r.b, "list-panes -a", before).await;
    stops_on(&mut r, "INT", "setting its status").await;
}

/// SIGTERM and SIGHUP stop a host the same way as Ctrl-C
/// (01M3NBV405PVYHKTMQ5VN87FYN).
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_and_sighup_stop_the_host() {
    let mut r = riff().await;
    stops_on(&mut r, "TERM", "waiting for a wake").await;
    let mut r = riff().await;
    stops_on(&mut r, "HUP", "waiting for a wake").await;
}

/// Ctrl-C stops a host that tries again after an error: its server does
/// not answer (01M3NBV405PVYHKTMQ5VN87FYN).
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_stops_a_host_that_tries_again() {
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}", free.local_addr().unwrap());
    drop(free);
    let b = Machine::new("b", &dead);
    b.limit(1);
    let mut host = b.host(&main);
    until("the host tries again", || async {
        b.host_output()
            .contains(riff::api::RECONNECTING)
            .then_some(())
    })
    .await;
    let took = host.stop("INT");
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

/// Ctrl-C stops a host at start, while the OS keyring does not answer
/// (01M3NBV405PVYHKTMQ5VN87FYN, 01M3NBTZDT67WD9ZX0RHDVCW9T).
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_stops_a_host_whose_keyring_does_not_answer() {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let b = Machine::new("b", api.base());
    let home = b.home.path();
    // A bus that takes each connection and never answers.
    let bus = home.join("bus");
    let _deaf = std::os::unix::net::UnixListener::bind(&bus).unwrap();
    std::fs::create_dir_all(home.join("config/riff")).unwrap();
    std::fs::write(
        home.join("config/riff/config.toml"),
        "[workers]\nlimit = 1\n",
    )
    .unwrap();
    let mut cmd = b.riff(&main, &["host", "--claude", "true"], None);
    cmd.env_remove("RIFF_HOME")
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", bus.display()),
        );
    let mut host = b.host_with(cmd);
    until("the start line", || async {
        b.host_output().contains("Ctrl-C stops it.").then_some(())
    })
    .await;
    std::thread::sleep(Duration::from_millis(300));
    assert!(host.0.try_wait().unwrap().is_none(), "{}", b.host_output());
    let took = host.stop("INT");
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

/// The first line of the host names the host, its limit, the lead that
/// it serves and the repository (01M3NBV4294DS3WZFEKR7M3PNF).
#[tokio::test(flavor = "multi_thread")]
async fn the_start_line_names_the_host_the_limit_the_lead_and_the_repository() {
    let r = riff().await;
    let place = identity::place_in(&r.main, "b").unwrap();
    let output = r.b.host_output();
    assert_eq!(
        output.lines().next(),
        Some(
            format!(
                "riff workers host: b offers 3 workers to the lead of mike in {}. \
                 Ctrl-C stops it.",
                place.repo_text()
            )
            .as_str()
        ),
        "{output}"
    );
}

/// A second host of the same user and repository on the machine refuses
/// to start, and names the first (01M3NBV44GKAX6WS391PN6R72W).
#[tokio::test(flavor = "multi_thread")]
async fn a_second_host_refuses_to_start() {
    let r = riff().await;
    let out =
        r.b.riff(&r.main, &["host", "--claude", "true"], None)
            .stdin(Stdio::null())
            .output()
            .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    let first = format!(
        "runs on b already: process {}, session {}.",
        r.host.0.id(),
        r.host_id
    );
    assert!(err.contains(&first), "{err}");
    assert!(!r.b.log().contains("window"), "{}", r.b.log());
}

/// The host reads no input: the offset of its input file stays at 0
/// after it answers a request and starts a worker with `tmux`
/// (01M3NBV46R0VB0JQNQ1ERG16J6).
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn the_host_reads_no_input() {
    let r = riff().await;
    r.api
        .tell(&r.lead, &r.host_id, "workers start 1")
        .await
        .unwrap();
    reads(&r.api, &r.lead, "b: started 1 worker").await;
    let fdinfo = format!("/proc/{}/fdinfo/0", r.host.0.id());
    let info = std::fs::read_to_string(fdinfo).unwrap();
    assert!(info.starts_with("pos:\t0\n"), "{info}");
}

static MOCK_KEYRING: std::sync::Once = std::sync::Once::new();

/// The sign-in of a pair from the server.
fn signed(pair: riff_core::wire::TokenReply) -> riff::login::SignIn {
    riff::login::SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: u64::MAX,
        riff_id: None,
    }
}

/// Signs in mike at `service` with a new device key, and keeps both in
/// `dir`, the secret files of a `RIFF_HOME`.
fn sign_in_files(service: &riff_server::Service, url: &str, dir: &Path) {
    let key = riff_core::dpop::Key::generate();
    riff::secrets::file_set(dir, &riff::device::secret_name(url), &key.to_secret()).unwrap();
    let pair = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let json = serde_json::to_string(&signed(pair)).unwrap();
    riff::secrets::file_set(dir, &riff::login::secret_name(url), &json).unwrap();
}

/// A host against a server with sign-in shows in `riff who`, answers a
/// start request of the lead, and stops on Ctrl-C with its session
/// ended (01M3ND6R8YXN1KTRTRAV5A7F14). Before, its status waited for the
/// token of its watch, and it never showed.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_works_against_a_server_with_sign_in() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = riff_server::Service::new(riff_server::auth::Config {
        require_sign_in: true,
        ..riff_server::auth::Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    // The lead runs in this process, with its sign-in in the mock store.
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let person = Api::new(&url);
    let jkt = riff::device::key(&url).unwrap().thumbprint();
    let pair = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    riff::login::store(&url, &signed(pair)).unwrap();

    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    let api = person.signed_in(Some("l1")).unwrap();
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();

    let b = Machine::new("b", &url);
    sign_in_files(&service, &url, &b.home.path().join("secrets"));
    b.limit(2);
    let mut host = b.host(&main);
    let host_id = host_session(&api, &lead, "b").await;

    api.tell(&lead, &host_id, "workers start 1").await.unwrap();
    reads(&api, &lead, "b: started 1 worker").await;
    assert_eq!(b.workers().len(), 1, "{}", b.log());

    let took = host.stop("INT");
    assert!(took < Duration::from_secs(2), "took {took:?}");
    assert!(
        !b.host_output().contains("did not end in time"),
        "{}",
        b.host_output()
    );
}
