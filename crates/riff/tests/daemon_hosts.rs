//! The lead manages the daemon of another host (#1063,
//! 01M579Z9JE6HY9HG3PWR1RR0RQ to 01M579Z9VRD12FRK6DSYVKDHQB). Two fake
//! hosts, `a` and `b`, each with a `riff daemon` of its own home, serve
//! the same repository on one riff-server. The lead runs on `a`. Each
//! daemon command with `--host` reaches the daemon of the host that it
//! names, and only that one.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;
use riff::daemon::remote;
use riff_client::api::Api;
use riff_core::name::{SessionUri, Who};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState};

/// The bound of each wait. It only ends a test that hangs.
const WAIT: Duration = Duration::from_secs(60);

/// One host: its own riff home and its daemon.
struct Host {
    env: Isolated,
    name: &'static str,
    server: String,
    daemon: Option<Child>,
}

impl Host {
    fn new(name: &'static str, server: &str) -> Host {
        Host {
            env: Isolated::new(),
            name,
            server: server.into(),
            daemon: None,
        }
    }

    /// `riff ARGS` of mike on this host in `dir`, as the session
    /// `session`, or as the person.
    fn riff(&self, dir: &Path, args: &[&str], session: Option<&str>) -> Command {
        let mut cmd = self.env.riff();
        cmd.args(args)
            .current_dir(dir)
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", self.name)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER");
        match session {
            Some(id) => cmd.env("RIFF_SESSION", id),
            None => cmd.env_remove("RIFF_SESSION"),
        };
        cmd
    }

    async fn run(&self, dir: &Path, args: &[&str], session: Option<&str>) -> Output {
        let mut cmd = self.riff(dir, args, session);
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    fn log(&self) -> PathBuf {
        self.env.path().join("daemon.out")
    }

    fn output(&self) -> String {
        std::fs::read_to_string(self.log()).unwrap_or_default()
    }

    /// The folder of the daemon of this host.
    fn dir(&self) -> PathBuf {
        self.env.riff_home().join("daemon")
    }

    /// Starts the daemon of this host, and waits until it listens.
    async fn start(&mut self) {
        let out = std::fs::File::create(self.log()).unwrap();
        let child = self
            .riff(Path::new("/"), &["daemon"], None)
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        self.daemon = Some(child);
        until("the daemon listens", || async {
            self.output()
                .contains("The riff daemon listens on")
                .then_some(())
        })
        .await;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
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
    let place = riff::identity::place_in(dir, host).unwrap();
    SessionUri::new(Who::new(user, Some(id)).unwrap(), place)
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
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

/// The riff-server, the hosts `a` and `b` with their daemons, which
/// serve the repository `main`, and the lead on `a`.
struct Riff {
    api: Api,
    a: Host,
    b: Host,
    main: PathBuf,
    lead: SessionUri,
    _root: tempfile::TempDir,
}

async fn riff() -> Riff {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let mut a = Host::new("a", api.base());
    let mut b = Host::new("b", api.base());
    for host in [&mut a, &mut b] {
        host.start().await;
        // The person serves the lead from the main clone, once.
        let out = host.run(&main, &["daemon", "host"], None).await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains("The riff daemon serves the lead of"),
            "{out:?}"
        );
    }
    let riff = Riff {
        api,
        a,
        b,
        main,
        lead,
        _root: root,
    };
    for host in ["a", "b"] {
        riff.daemon_of(host).await;
    }
    riff
}

impl Riff {
    /// The session ID of the daemon of `host` in `who`.
    async fn daemon_of(&self, host: &str) -> String {
        until("the daemon in who", || async {
            let who = self.api.who(&self.lead, false).await.ok()?;
            let repo = self.lead.place().repo_text();
            remote::find(&who, "mike", &repo, host).map(str::to_owned)
        })
        .await
    }

    /// `riff ARGS --host HOST` of the lead on `a`.
    async fn lead(&self, args: &[&str], host: &str) -> Output {
        let mut all = args.to_vec();
        all.extend(["--host", host]);
        self.a.run(&self.main, &all, Some("l1")).await
    }

    fn host(&self, name: &str) -> &Host {
        if name == "a" { &self.a } else { &self.b }
    }
}

/// 01M579Z9JE6HY9HG3PWR1RR0RQ, 01M579Z9MPS86MX7890T6HV18S: each daemon
/// command with `--host` goes to the daemon of that host through
/// riff-server, and its reply comes back to the lead.
#[tokio::test(flavor = "multi_thread")]
async fn each_daemon_command_with_host_reaches_the_daemon_of_that_host() {
    let r = riff().await;
    for name in ["a", "b"] {
        let host = r.host(name);
        let dir = host.dir().display().to_string();

        let out = r.lead(&["daemon", "status"], name).await;
        let status = text(&out.stdout);
        assert!(out.status.success(), "{out:?}");
        assert!(status.contains(&format!("Its folder: {dir}")), "{status}");
        assert!(
            status.contains("The compile cache of the container sessions:"),
            "{status}"
        );

        // No --clear here: podman of the test machine is not isolated.
        let size = if name == "a" { "12G" } else { "13G" };
        let out = r.lead(&["daemon", "cache", size], name).await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains(&format!("at most {size}")),
            "{out:?}"
        );
        let saved = std::fs::read_to_string(host.dir().join("cache.json")).unwrap();
        assert!(saved.contains(size), "{saved}");
        let out = r.lead(&["daemon", "cache"], name).await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains(&format!("at most {size}")),
            "{out:?}"
        );
        let out = r.lead(&["daemon", "cache", "12GB"], name).await;
        assert!(!out.status.success(), "{out:?}");
        assert!(
            text(&out.stderr).contains("a number with K, M, G or T"),
            "{out:?}"
        );

        let sessions = if name == "a" { "4" } else { "5" };
        let out = r
            .lead(&["daemon", "limit", "--sessions", sessions], name)
            .await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains(&format!("{sessions} live sessions")),
            "{out:?}"
        );
        let saved = std::fs::read_to_string(host.dir().join("limit.json")).unwrap();
        assert!(
            saved.contains(&format!("\"sessions\":{sessions}")),
            "{saved}"
        );

        let out = r.lead(&["daemon", "sweep"], name).await;
        assert!(out.status.success(), "{out:?}");
        assert!(text(&out.stdout).contains("removed no folder"), "{out:?}");

        let out = r.lead(&["daemon", "restart"], name).await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains("runs the riff on disk"),
            "{out:?}"
        );

        // The daemon of the host says why it cannot.
        let none = "riff-issue-9-00000000";
        for args in [["daemon", "events", none], ["daemon", "end", none]] {
            let out = r.lead(&args, name).await;
            assert!(!out.status.success(), "{out:?}");
            assert!(
                text(&out.stderr).contains(&format!("owns no session in the container {none}")),
                "{out:?}"
            );
        }
        let out = r.lead(&["wave", "stop"], name).await;
        assert!(!out.status.success(), "{out:?}");
        assert!(
            text(&out.stderr).contains("does not run the wave"),
            "{out:?}"
        );
        // The main clone pins no session image: the daemon starts no wave.
        let out = r.lead(&["wave", "run"], name).await;
        assert!(!out.status.success(), "{out:?}");
        assert!(!text(&out.stderr).contains("no daemon"), "{out:?}");
    }
    // Each limit went to its own host.
    for (host, sessions) in [(&r.a, "4"), (&r.b, "5")] {
        let saved = std::fs::read_to_string(host.dir().join("limit.json")).unwrap();
        assert!(
            saved.contains(&format!("\"sessions\":{sessions}")),
            "{saved}"
        );
    }
    // Each reply is a note: the daemon says each one in its log.
    assert!(
        r.b.output().contains(", the daemon of b: done"),
        "{}",
        r.b.output()
    );
    assert!(
        r.b.output().contains(", the daemon of b: failed"),
        "{}",
        r.b.output()
    );

    // A host that serves no daemon.
    let out = r.lead(&["daemon", "status"], "c").await;
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("no daemon of your user on c"),
        "{out:?}"
    );
}

/// 01M579Z9PZZCVWQKG7V8WZ5KRB: the person acts on the daemon of a host
/// from a terminal. A worker session and another user get the refusal,
/// and change nothing.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_person_and_the_lead_act_on_the_daemon() {
    let r = riff().await;
    let out =
        r.a.run(
            &r.main,
            &["daemon", "limit", "--sessions", "6", "--host", "b"],
            None,
        )
        .await;
    assert!(out.status.success(), "{out:?}");
    assert!(text(&out.stdout).contains("6 live sessions"), "{out:?}");

    let worker = session(&r.main, "mike", "a", "w1");
    r.api.register(&worker).await.unwrap();
    let out =
        r.a.run(
            &r.main,
            &["daemon", "limit", "--sessions", "1", "--host", "b"],
            Some("w1"),
        )
        .await;
    assert!(!out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains("refused: \"daemon limit --sessions 1\" is not from mike"),
        "{out:?}"
    );

    // Another user asks the daemon of mike on b.
    let brett = session(&r.main, "brett", "k", "b1");
    r.api.register(&brett).await.unwrap();
    let daemon = r.daemon_of("b").await;
    let thread = brett.default_thread().unwrap();
    let posted = r
        .api
        .post(
            &brett,
            Some(&thread),
            &[Selector::session(&daemon)],
            "daemon limit --sessions 2",
            Kind::Message,
        )
        .await
        .unwrap();
    let reply = until("the reply to brett", || async {
        let read = r.api.read_after(&brett, &thread, posted.seq).await.ok()?;
        read.iter()
            .find_map(|m| remote::read_reply(&m.message.body, posted.seq))
    })
    .await;
    assert!(
        reply
            .unwrap_err()
            .contains("refused: \"daemon limit --sessions 2\" is not from mike"),
    );
    let saved = std::fs::read_to_string(r.b.dir().join("limit.json")).unwrap();
    assert!(saved.contains("\"sessions\":6"), "{saved}");
}

/// 01M579Z9JE6HY9HG3PWR1RR0RQ: after a new start, the daemon serves
/// with the same session, and shows its new pid.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_daemon_serves_with_the_same_session() {
    let mut r = riff().await;
    let before = r.daemon_of("b").await;
    let out = r.lead(&["daemon", "status"], "b").await;
    let old = text(&out.stdout);
    let mut child = r.b.daemon.take().unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    r.b.start().await;
    let after = r.daemon_of("b").await;
    assert_eq!(before, after);
    let new = until("the status of the new daemon", || async {
        let out = r.lead(&["daemon", "status"], "b").await;
        let new = text(&out.stdout);
        (out.status.success() && new != old).then_some(new)
    })
    .await;
    let pid = |s: &str| {
        s.split("pid ")
            .nth(1)
            .map(|p| p.split(',').next().unwrap().to_owned())
    };
    assert_ne!(pid(&old), pid(&new), "{old}\n{new}");
}
