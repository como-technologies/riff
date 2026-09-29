//! Workers on another machine of the user (01M3N7AK8TVYV8S0WR3RP0TN8X
//! to 01M3N7AKFPX3ZGQARSG2V64GBD). Two machines, `a` and `b`, each with
//! a fake `tmux` on `PATH` that writes each call to a log and keeps the
//! worker panes in a file. The lead runs on `a`; `riff workers host`
//! runs on `b`.

use isolated::Isolated;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff::identity;
use riff_core::name::{SessionUri, Who};
use riff_core::wire::{RiffState, Status};

/// `list-panes -a` lists the worker panes with their session marks, from
/// the file `workers`. `kill-pane` removes a pane from it.
const FAKE_TMUX: &str = r#"#!/bin/sh
dir=$(dirname "$0")
printf '%s\n' "$*" >> "$dir/log"
n=$(grep -c -e '^split-window' -e '^new-window' "$dir/log")
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
    mv "$dir/workers.new" "$dir/workers" ;;
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
        let path = format!(
            "{}:{}",
            self.fake.path().display(),
            std::env::var("PATH").unwrap()
        );
        let mut cmd = Isolated::shared().riff();
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
        let child = self
            .riff(dir, &["host", "--claude", "true"], None)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Running(child)
    }
}

/// A process that is killed on drop.
struct Running(Child);

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
    _host: Running,
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
    host_session(&api, &lead, "b").await;
    Riff {
        api,
        a,
        b,
        main,
        lead,
        _root: root,
        _host: host,
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
    assert!(read.contains("direct with mike@b"), "{read}");

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
        listed.starts_with("No worker runs on this machine."),
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
