//! The lead manages the daemon of another host (#1063,
//! 01M579Z9JE6HY9HG3PWR1RR0RQ to 01M579Z9VRD12FRK6DSYVKDHQB), and sees
//! the facts of each host (#1065, 01M57D3F50347RD8ED79KEA9V7 to
//! 01M57D3FGV57W96A7SWTEFRCGW). Two fake hosts, `a` and `b`, each with a
//! `riff daemon` of its own home, serve the same repository on one
//! riff-server. The lead runs on `a`. Each daemon command with `--host`
//! reaches the daemon of the host that it names, and only that one.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use isolated::Isolated;
use riff::daemon::{facts, remote};
use riff_client::api::Api;
use riff_core::name::{SessionUri, Who};
use riff_core::selector::Selector;
use riff_core::wire::{Kind, RiffState};

/// The bound of each wait. It only ends a test that hangs.
const WAIT: Duration = Duration::from_secs(60);

/// The time between two looks of each daemon at its facts, in
/// milliseconds.
const LOOK_MS: u64 = 200;

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
    /// `session`, or as the person. Each host has numbers of its own:
    /// its machine and its disk. Its daemon sends its facts each
    /// [`LOOK_MS`].
    fn riff(&self, dir: &Path, args: &[&str], session: Option<&str>) -> Command {
        let mut cmd = self.env.riff();
        let (cores, free) = if self.name == "a" {
            (16, 100)
        } else {
            (32, 300)
        };
        cmd.args(args)
            .current_dir(dir)
            .env("RIFF_SERVER", &self.server)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", self.name)
            .env(
                "RIFF_MACHINE",
                format!("cpu {cores}x3000MHz, mem 64GB, 60GB available, load 0.50"),
            )
            .env("RIFF_DISK", format!("disk {free}GB free of 400GB"))
            .env(facts::EVERY_VAR, LOOK_MS.to_string())
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
    riff_with(300, 300).await
}

/// As [`riff`], with a sign-in of claude on `a` and on `b` that ends in
/// these minutes.
async fn riff_with(a_minutes: i64, b_minutes: i64) -> Riff {
    let api = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    api.register(&lead).await.unwrap();
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let mut a = Host::new("a", api.base());
    let mut b = Host::new("b", api.base());
    a.claude_ends_in(a_minutes);
    b.claude_ends_in(b_minutes);
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
/// and change nothing. 01M588QXAGD3B8BTK7N17M29WM: the refusal names
/// the session or the user, and the cause.
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
        text(&out.stderr).contains(
            "refused: \"daemon limit --sessions 1\" comes from the session w1 of mike, which is \
             not the lead of mike in"
        ),
        "{out:?}"
    );
    // A status only reads, and gets the same refusal.
    let out =
        r.a.run(&r.main, &["daemon", "status", "--host", "b"], Some("w1"))
            .await;
    assert!(!out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(
            "refused: \"daemon status\" comes from the session w1 of mike, which is not the \
             lead of mike in"
        ),
        "{out:?}"
    );
    assert!(
        text(&out.stderr).contains("Only mike and the lead act on the daemon of b."),
        "{out:?}"
    );

    // Another user asks the daemon of mike on b.
    let brett = session(&r.main, "brett", "k", "b1");
    r.api.register(&brett).await.unwrap();
    let daemon = r.daemon_of("b").await;
    // A direct message, as riff sends it (01M57D4FTP2NXAE18XFRXCJ79H).
    let posted = r
        .api
        .post(
            &brett,
            None,
            &[Selector::session(&daemon)],
            "daemon limit --sessions 2",
            Kind::Message,
        )
        .await
        .unwrap();
    let reply = until("the reply to brett", || async {
        let read = r
            .api
            .read_after(&brett, &posted.thread, posted.seq)
            .await
            .ok()?;
        read.iter()
            .find_map(|m| remote::read_reply(&m.message.body, posted.seq))
    })
    .await;
    assert!(reply.unwrap_err().contains(
        "refused: \"daemon limit --sessions 2\" comes from the session b1 of brett, not from mike."
    ),);
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

impl Host {
    /// Writes the sign-in file of `claude` of this host: it ends in
    /// `minutes`.
    fn claude_ends_in(&self, minutes: i64) {
        let dir = self.env.home().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let at = i64::try_from(now).unwrap() + minutes * 60_000;
        let file = format!(
            r#"{{"claudeAiOauth":{{"accessToken":"sk-a","refreshToken":"sk-r","expiresAt":{at}}}}}"#
        );
        std::fs::write(dir.join(".credentials.json"), file).unwrap();
    }
}

impl Riff {
    /// The bodies of the notes of the daemons to the lead that start
    /// with `start`.
    async fn notes(&self, start: &str) -> Vec<String> {
        let thread = self.lead.default_thread().unwrap();
        let read = self.api.read_after(&self.lead, &thread, 0).await.unwrap();
        read.into_iter()
            .filter(|m| m.message.kind == Kind::Note)
            .map(|m| m.message.body)
            .filter(|body| body.starts_with(start))
            .collect()
    }
}

/// 01M57D3F50347RD8ED79KEA9V7, 01M57D3F7FXTPZB3M72Z1DQ1T7,
/// 01M57D3F9SWMKP22WNRPRY3X5T, 01M57D3FGV57W96A7SWTEFRCGW,
/// 01M58B7HA8CRCW0MJ17QP3R5QG: the daemon of
/// each host sends its facts, and `riff who` and `riff top` of the lead
/// show each fact for the right host.
#[tokio::test(flavor = "multi_thread")]
async fn riff_who_and_riff_top_show_the_facts_of_each_host() {
    let r = riff_with(300, 600).await;
    // A token on b only (01M58B7HA8CRCW0MJ17QP3R5QG).
    riff::claude_token::store(&r.b.dir(), "sk-ant-oat01-secret1").unwrap();
    let out = r.lead(&["daemon", "limit", "--sessions", "5"], "b").await;
    assert!(out.status.success(), "{out:?}");
    let who = until("the facts of each host in riff who", || async {
        let out = r.a.run(&r.main, &["who"], Some("l1")).await;
        let who = text(&out.stdout);
        let fresh = who.contains("sessions 0/5")
            && who.contains("claude token set")
            && who.contains("claude token not set");
        (out.status.success() && fresh).then_some(who)
    })
    .await;
    let part = |host: &str| {
        let start = who.find(&format!("mike@{host} (")).unwrap();
        let rest = &who[start..];
        let end = rest[1..].find("mike@").map_or(rest.len(), |at| at + 1);
        rest[..end].to_owned()
    };
    let (a, b) = (part("a"), part("b"));
    let build = riff_core::build::Build::this().to_string();
    let build: Vec<&str> = build.split_whitespace().take(2).collect();
    let build = build.join(" ");
    for (host, part, cores, free, limit, left, token) in [
        (
            "a",
            &a,
            16,
            100,
            "sessions 0/",
            "claude 4h",
            "claude token not set",
        ),
        (
            "b",
            &b,
            32,
            300,
            "sessions 0/5",
            "claude 9h",
            "claude token set",
        ),
    ] {
        assert!(part.contains(&format!("daemon {build}")), "{host}: {part}");
        assert!(part.contains(limit), "{host}: {part}");
        assert!(
            part.contains(&format!("load 0.5/{cores}")),
            "{host}: {part}"
        );
        assert!(part.contains("60GB avail"), "{host}: {part}");
        assert!(
            part.contains(&format!("disk {free}GB free")),
            "{host}: {part}"
        );
        assert!(part.contains("no session image"), "{host}: {part}");
        assert!(part.contains("riff on"), "{host}: {part}");
        // The gh of a test has no sign-in.
        assert!(part.contains("gh off"), "{host}: {part}");
        assert!(part.contains(left), "{host}: {part}");
        assert!(part.contains(token), "{host}: {part}");
    }
    assert!(!a.contains("sessions 0/5"), "{a}");
    assert!(!who.contains("sk-ant-oat01-secret1"), "{who}");

    let out =
        r.a.run(&r.main, &["top", "--once", "--host", "b"], Some("l1"))
            .await;
    assert!(out.status.success(), "{out:?}");
    let top = text(&out.stdout);
    assert!(top.contains("load 0.5/32 threads"), "{top}");
    assert!(top.contains("sessions 0/5"), "{top}");
    assert!(!top.contains("load 0.5/16 threads"), "{top}");
}

/// 01M57D3FC5PB5EQ6Y38Y5R2DBB: the lead gets one note when the sign-in
/// of claude on a host has less than 2 hours left, not one at each look,
/// and one note when it comes back.
#[tokio::test(flavor = "multi_thread")]
async fn a_short_sign_in_of_claude_is_one_note_to_the_lead() {
    let r = riff_with(300, 30).await;
    let short = "The sign-in of claude on b ends in";
    until("the note of the short sign-in", || async {
        (!r.notes(short).await.is_empty()).then_some(())
    })
    .await;
    // Many looks later: still one note.
    tokio::time::sleep(Duration::from_millis(LOOK_MS * 10)).await;
    let notes = r.notes(short).await;
    assert_eq!(notes.len(), 1, "{notes:#?}");
    assert!(notes[0].contains("run claude on b"), "{notes:#?}");
    assert!(r.notes("The sign-in of claude on a").await.is_empty());

    r.b.claude_ends_in(600);
    let back = "The sign-in of claude on b is back";
    until("the note of the sign-in that came back", || async {
        (!r.notes(back).await.is_empty()).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(LOOK_MS * 10)).await;
    assert_eq!(r.notes(back).await.len(), 1);
    assert_eq!(r.notes(short).await.len(), 1);
}

/// 01M57D4FTP2NXAE18XFRXCJ79H, 01M57D4FR8TPY47WPT9G66H4PE: the person
/// on `a` asks the daemon of `b` from a terminal, and gets the reply in
/// the direct thread of the person. The thread of the repository has no
/// reply, so another user in it reads none.
#[tokio::test(flavor = "multi_thread")]
async fn the_replies_of_a_daemon_go_only_to_the_person_and_the_lead() {
    let r = riff().await;
    let none = "riff-issue-9-00000000";
    let out =
        r.a.run(&r.main, &["daemon", "events", none, "--host", "b"], None)
            .await;
    assert!(!out.status.success(), "{out:?}");
    assert!(
        text(&out.stderr).contains(&format!("owns no session in the container {none}")),
        "{out:?}"
    );
    let out = r.lead(&["daemon", "status"], "b").await;
    assert!(out.status.success(), "{out:?}");

    let brett = session(&r.main, "brett", "k", "b1");
    r.api.register(&brett).await.unwrap();
    let thread = brett.default_thread().unwrap();
    let read = r.api.read(&brett, &thread, true).await.unwrap_or_default();
    assert!(
        read.iter()
            .all(|m| !m.message.body.contains("the daemon of b")),
        "{read:?}"
    );
    let threads = r.api.threads(&brett).await.unwrap();
    assert!(threads.iter().all(|t| !t.thread.is_direct()), "{threads:?}");
    // The lead has its direct thread with the daemon of b.
    let daemon = r.daemon_of("b").await;
    let direct = riff_core::name::ThreadName::direct(
        r.lead.who(),
        &Who::new("mike", Some(&daemon)).unwrap(),
    );
    let read = r.api.read(&r.lead, &direct, true).await.unwrap();
    assert!(
        read.iter()
            .any(|m| m.message.body.contains("the daemon of b: done")),
        "{read:?}"
    );
}
