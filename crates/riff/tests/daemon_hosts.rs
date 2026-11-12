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

/// The tag of the release that the real-session test checks.
const TAG: &str = "v9.9.9";

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
    /// More variables for each command and for the daemon.
    extra: Vec<(String, String)>,
}

impl Host {
    fn new(name: &'static str, server: &str) -> Host {
        Host {
            env: Isolated::new(),
            name,
            server: server.into(),
            daemon: None,
            extra: Vec::new(),
        }
    }

    /// `riff ARGS` of mike on this host in `dir`, as the session
    /// `session`, or as the person. Each host has numbers of its own:
    /// its machine and its disk. Its daemon sends its facts each
    /// [`LOOK_MS`].
    fn riff(&self, dir: &Path, args: &[&str], session: Option<&str>) -> Command {
        crate::common::sign_in(&self.server, &self.env.riff_home(), "mike");
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
            .env(riff::watches::EVERY_VAR, LOOK_MS.to_string())
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("RIFF_WORKER")
            .envs(self.extra.iter().map(|(k, v)| (k, v)));
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

    /// Ends the daemon of this host, as a restart does: its files stay.
    fn stop(&mut self) {
        if let Some(mut child) = self.daemon.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// What a lead on this host leaves in the folder of its daemon (#1643):
    /// the daemon serves the repository of `main`, and the host has the part
    /// lead and not the part workers. The daemon of a host that never
    /// joined serves a repository only for its lead.
    fn serve_lead(&self, main: &Path) {
        let repo = riff::identity::place_in(main, self.name)
            .unwrap()
            .repo_text();
        std::fs::create_dir_all(self.dir()).unwrap();
        let wave = serde_json::json!({
            "repo": repo, "main": main, "server": &self.server, "host": self.name,
        });
        let session = riff::identity::new_session_id();
        let served = serde_json::json!({repo.clone(): {"wave": wave, "session": session}});
        std::fs::write(self.dir().join("served.json"), served.to_string()).unwrap();
        let parts = serde_json::json!({repo: {"workers": false, "lead": true}});
        std::fs::write(self.dir().join("parts.json"), parts.to_string()).unwrap();
    }

    /// Starts the daemon of this host, and waits until it listens. As a
    /// systemd unit, it sets no `RIFF_SERVER`: the daemon takes the
    /// riff-server of each repository from `served.json`.
    async fn start(&mut self) {
        let out = std::fs::File::create(self.log()).unwrap();
        let child = self
            .riff(Path::new("/"), &["daemon"], None)
            .env_remove("RIFF_SERVER")
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

async fn start_server() -> crate::common::Signed {
    crate::common::start_signed().await
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
    // 01M5SMPX8NM75P8E38DGQ17W7B: the release cycle is for a repository
    // with the table `[release]`.
    std::fs::write(main.join("riff.toml"), "[release]\n").unwrap();
    git(&["add", "riff.toml"]);
    git(&["commit", "-q", "-m", "x"]);
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
    server: crate::common::Signed,
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
    riff_env(a_minutes, b_minutes, &[]).await
}

/// As [`riff_with`], with more variables for each host.
async fn riff_env(a_minutes: i64, b_minutes: i64, extra: &[(String, String)]) -> Riff {
    riff_on(start_server().await, (a_minutes, b_minutes), extra).await
}

/// As [`riff_env`], on `server`, with a sign-in of claude on `a` and on
/// `b` that ends in the `minutes`.
async fn riff_on(
    server: crate::common::Signed,
    minutes: (i64, i64),
    extra: &[(String, String)],
) -> Riff {
    let (a_minutes, b_minutes) = minutes;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    let api = |me: &SessionUri| crate::common::api_of(&server, me);
    api(&lead).register(&lead).await.unwrap();
    crate::common::name_lead(&server.api("mike", None), &lead).await;
    api(&lead)
        .set_riff(&lead, RiffState::Running)
        .await
        .unwrap();
    let mut a = Host::new("a", &server);
    let mut b = Host::new("b", &server);
    a.extra = extra.to_vec();
    b.extra = extra.to_vec();
    a.claude_ends_in(a_minutes);
    b.claude_ends_in(b_minutes);
    for host in [&mut a, &mut b] {
        // A lead runs on the host: it serves the repository for the lead.
        host.serve_lead(&main);
        host.start().await;
    }
    let riff = Riff {
        server,
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
    /// A client of the server for `me`: the token of its user and session.
    fn api(&self, me: &SessionUri) -> Api {
        crate::common::api_of(&self.server, me)
    }

    /// The session ID of the daemon of `host` in `who`.
    async fn daemon_of(&self, host: &str) -> String {
        until("the daemon in who", || async {
            let who = self.api(&self.lead).who(&self.lead, false).await.ok()?;
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
        let out = r.lead(&["daemon", "cache"], name).await;
        assert!(out.status.success(), "{out:?}");
        let shown = text(&out.stdout);
        assert!(
            shown.contains("The compile cache of the container sessions:"),
            "{out:?}"
        );
        assert!(!shown.contains("at most"), "{out:?}");
        // riff keeps no size: it is in the riff.toml of the repository
        // (01M5SMW28KT5D5HPRH7KAZ8TVC).
        let out = r.lead(&["daemon", "cache", "12G"], name).await;
        assert!(!out.status.success(), "{out:?}");
        assert!(!host.dir().join("cache.json").exists());

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

        let out = r
            .lead(&["daemon", "limit", "--effort-lead", "max"], name)
            .await;
        assert!(out.status.success(), "{out:?}");
        assert!(
            text(&out.stdout).contains("Effort of the lead max"),
            "{out:?}"
        );
        let saved = std::fs::read_to_string(host.dir().join("limit.json")).unwrap();
        assert!(saved.contains("\"effort_lead\":\"max\""), "{saved}");

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
        let out = r.lead(&["leave"], name).await;
        assert!(!out.status.success(), "{out:?}");
        // The host has the part lead only: it gives no workers to leave.
        assert!(
            text(&out.stderr).contains("gives no workers to local/riff"),
            "{out:?}"
        );
        // The main clone pins no session image: the daemon starts no wave.
        let out = r.lead(&["join"], name).await;
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
    r.api(&worker).register(&worker).await.unwrap();
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
    r.api(&brett).register(&brett).await.unwrap();
    let daemon = r.daemon_of("b").await;
    // A direct message, as riff sends it (01M57D4FTP2NXAE18XFRXCJ79H).
    let posted = r
        .api(&brett)
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
            .api(&brett)
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
    /// The bodies of the messages to the lead that start with `start`.
    async fn messages(&self, start: &str) -> Vec<String> {
        let thread = self.lead.default_thread().unwrap();
        let read = self
            .api(&self.lead)
            .read_after(&self.lead, &thread, 0)
            .await
            .unwrap();
        read.into_iter()
            .filter(|m| m.message.kind == Kind::Message)
            .map(|m| m.message.body)
            .filter(|body| body.starts_with(start))
            .collect()
    }

    /// The bodies of the notes of the daemons to the lead that start
    /// with `start`.
    async fn notes(&self, start: &str) -> Vec<String> {
        let thread = self.lead.default_thread().unwrap();
        let read = self
            .api(&self.lead)
            .read_after(&self.lead, &thread, 0)
            .await
            .unwrap();
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
    let secrets = r.b.env.riff_home().join("secrets");
    riff_client::secrets::file_set(&secrets, riff::claude_token::NAME, "sk-ant-oat01-secret1")
        .unwrap();
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
        // The image is on the line of a wave, and no wave runs here
        // (01M5TKPG5WZ1T58FNAMZNP6CJ5).
        assert!(!part.contains("image"), "{host}: {part}");
        assert!(part.contains("riff on"), "{host}: {part}");
        // The parts of the host in the repository (#1643).
        assert!(part.contains("parts local/riff lead"), "{host}: {part}");
        // The gh of a test has no sign-in, also in a session container
        // (01M6XWAC88HABQRKPG9MM1SCY0).
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

/// #1858, 01M749XKZ7N13R6EDJEREZZ0F4, 01M749XM23FTKZ95QEZCMVQ2K5: while
/// riff-server refuses the facts of the hosts, the daemon of each host
/// logs one warning line with the cause, also in its own log file, and
/// `riff daemon status` says why. When riff-server takes them again, the
/// daemon logs one line, and `riff who` lists the host.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_send_of_the_facts_is_one_warning_line_with_its_cause() {
    use axum::extract::Request;
    use axum::middleware::{Next, from_fn};
    use axum::response::IntoResponse;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};

    let (refuse, refused) = (
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicUsize::new(0)),
    );
    let server = {
        let (refuse, refused) = (refuse.clone(), refused.clone());
        crate::common::start_signed_with(move |router| {
            router.layer(from_fn(move |request: Request, next: Next| {
                let (refuse, refused) = (refuse.clone(), refused.clone());
                async move {
                    if request.uri().path() == "/v1/machine" && refuse.load(SeqCst) {
                        refused.fetch_add(1, SeqCst);
                        let build = riff_core::build::Build::this().to_string();
                        let status = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
                        let header = [(riff_core::build::HEADER, build)];
                        return (status, header, "the test refuses the facts").into_response();
                    }
                    next.run(request).await
                }
            }))
        })
        .await
    };
    let r = riff_on(server, (300, 300), &[]).await;
    let warnings = |host: &Host| -> Vec<String> {
        let out = host.output();
        let lines = out.lines().filter(|l| l.contains("cannot send the facts"));
        lines.map(str::to_owned).collect()
    };
    until("a warning of each host", || async {
        let warned = !warnings(&r.a).is_empty() && !warnings(&r.b).is_empty();
        (warned && refused.load(SeqCst) >= 10).then_some(())
    })
    .await;
    // Many looks later: still one warning line.
    tokio::time::sleep(Duration::from_millis(LOOK_MS * 5)).await;
    let repo = r.lead.place().repo_text();
    for host in [&r.a, &r.b] {
        let lines = warnings(host);
        assert_eq!(lines.len(), 1, "{}", host.output());
        let want = riff::text::daemon_facts_failed(
            &repo,
            "machine failed (500 Internal Server Error): the test refuses the facts",
        );
        assert_eq!(lines[0], format!("riff daemon: {want}"));
        // The log file of the daemon has the line, after its time.
        if cfg!(target_os = "linux") {
            let path = host.dir().join(riff::daemon::log::FILE);
            let own = std::fs::read_to_string(&path).unwrap();
            let line = own.lines().find(|l| l.contains("cannot send the facts"));
            let line = line.unwrap_or_else(|| panic!("{own}"));
            let (time, rest) = line.split_once(' ').unwrap();
            assert!(chrono::DateTime::parse_from_rfc3339(time).is_ok(), "{line}");
            assert_eq!(rest, lines[0]);
        }
    }
    let out = r.b.run(&r.main, &["daemon", "status"], None).await;
    let status = text(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(
        status.contains(&format!("of the facts of this host for {repo} failed in")),
        "{status}"
    );
    assert!(
        status.contains("The last cause: machine failed"),
        "{status}"
    );

    refuse.store(false, SeqCst);
    until("b in the part Hosts of riff who", || async {
        let out = r.a.run(&r.main, &["who"], Some("l1")).await;
        let who = text(&out.stdout);
        let hosts = who.split_once("\nHosts:\n")?.1.to_owned();
        hosts.contains("mike@b (").then_some(())
    })
    .await;
    until("the line of the facts that go out again", || async {
        let again = "the facts of this host go out again, after";
        r.b.output().contains(again).then_some(())
    })
    .await;
    assert_eq!(warnings(&r.b).len(), 1, "{}", r.b.output());
    let out = r.b.run(&r.main, &["daemon", "status"], None).await;
    let status = text(&out.stdout);
    assert!(!status.contains("Facts:"), "{status}");
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
    r.api(&brett).register(&brett).await.unwrap();
    let thread = brett.default_thread().unwrap();
    let read = r
        .api(&brett)
        .read(&brett, &thread, true)
        .await
        .unwrap_or_default();
    assert!(
        read.iter()
            .all(|m| !m.message.body.contains("the daemon of b")),
        "{read:?}"
    );
    let threads = r.api(&brett).threads(&brett).await.unwrap();
    assert!(threads.iter().all(|t| !t.thread.is_direct()), "{threads:?}");
    // The lead has its direct thread with the daemon of b.
    let daemon = r.daemon_of("b").await;
    let direct = riff_core::name::ThreadName::direct(
        r.lead.who(),
        &Who::new("mike", Some(&daemon)).unwrap(),
    );
    let read = r.api(&r.lead).read(&r.lead, &direct, true).await.unwrap();
    assert!(
        read.iter()
            .any(|m| m.message.body.contains("the daemon of b: done")),
        "{read:?}"
    );
}

/// A `gh` that logs its arguments in `log`, and a test that writes its
/// `RIFF_SERVER` in `dir/server`, waits while the file `dir/hold`
/// exists, and ends with the code in `code`, in `dir`. Gives the
/// variables for a host.
fn fake_real_session(dir: &Path, log: &Path, code: &Path) -> Vec<(String, String)> {
    let script = |name: &str, body: String| {
        isolated::write_program(dir.join(name), format!("#!/bin/sh\n{body}\n"))
    };
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let gh = script(
        "gh",
        format!(
            "echo \"$*\" >> {}\n\nif [ \"$1 $2\" = \"issue list\" ]; then echo '[{{\"number\":7,\"title\":\"Release {TAG}\"}}]'; fi\nif [ \"$1 $2\" = \"issue comment\" ]; then cat >> {}; fi\n\
             case \"$*\" in\n\
             *statuses/*) for a in \"$@\"; do case \"$a\" in state=*) echo \"${{a#state=}}\" > {}; esac; done ;;\n\
             'api repos/{{owner}}/{{repo}}/commits/'*'/status') printf '{{\"statuses\":[{{\"context\":\"riff/real-session\",\"state\":\"%s\"}}]}}\\n' \"$(cat {} 2>/dev/null)\" ;;\n\
             esac",
            log.display(),
            dir.join("comment").display(),
            dir.join("state").display(),
            dir.join("state").display()
        ),
    );
    std::fs::rename(gh, bin.join("gh")).unwrap();
    let run = script(
        "run",
        format!(
            "printf %s \"$RIFF_SERVER\" > {}\necho \"a line\"\necho \"GH_TOKEN=ghp_secret1\"\necho \"real-session: FAIL at step 6 of 10, run the item: exit 1\" >&2\nwhile [ -e {} ]; do sleep 0.1; done\nexit $(cat {})",
            dir.join("server").display(),
            dir.join("hold").display(),
            code.display()
        ),
    );
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    vec![
        ("PATH".into(), path),
        (
            riff::daemon::release_test::RUN_VAR.into(),
            run.display().to_string(),
        ),
    ]
}

/// 01M5924C8BTT6CW05Q0YZJ1GCY: the lead asks the daemon of a host to
/// run the real-session test on a commit. The daemon sets the status
/// pending, runs the test in a clone, and sets success on a pass and
/// failure on a fail. A work session gets the refusal.
/// 01M5MR8CM887EFGMKZM5BQA9JJ: the daemon has no `RIFF_SERVER`, and the
/// test gets the riff-server of the repository.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_runs_the_real_session_test_and_sets_the_status() {
    let tmp = tempfile::tempdir().unwrap();
    let (log, code) = (tmp.path().join("gh.log"), tmp.path().join("code"));
    let extra = fake_real_session(tmp.path(), &log, &code);
    let r = riff_env(300, 300, &extra).await;
    // The main clone has an origin that the daemon clones.
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        text(&out.stdout).trim().to_owned()
    };
    let bare = tmp.path().join("origin.git");
    git(
        tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &r.main.display().to_string(),
            "origin.git",
        ],
    );
    git(
        &r.main,
        &["remote", "add", "origin", &bare.display().to_string()],
    );
    // 01M5TKEH5E2ZQ22PAN9FTT27VW: the daemon reads the merged `riff.toml`
    // of the default branch of `origin`.
    git(&r.main, &["fetch", "-q", "origin"]);
    git(&r.main, &["remote", "set-head", "origin", "--auto"]);
    let commit = git(&r.main, &["rev-parse", "HEAD"]);

    let hold = tmp.path().join("hold");
    for (host, exit, state) in [("a", "0", "success"), ("b", "1", "failure")] {
        std::fs::write(&code, exit).unwrap();
        std::fs::write(&hold, "").unwrap();
        let asked = remote::Request::ReleaseTest {
            commit: commit.clone(),
            tag: TAG.to_owned(),
        };
        let reply = remote::ask(
            &r.api(&r.lead),
            &r.lead,
            host,
            &asked,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert!(reply.contains("started on"), "{reply}");
        // 01M5RGCW5NCKQYA61RM63914TX: the request registered the watch of the
        // test on the daemon of the host.
        let line = format!("riff watches the real-session test of {TAG} on {commit}.");
        assert!(reply.contains(&line), "{reply}");
        let dir = r.host(host).dir();
        let watches = riff::watches::Watches::new(&dir).read().unwrap();
        assert_eq!(watches.len(), 1, "{watches:?}");
        let op = riff::watches::Op::Test {
            tag: TAG.to_owned(),
            commit: commit.clone(),
        };
        assert_eq!(watches[0].op, op);
        // 01M5QVBTXE8YRY44A1C1VP71QS: the status of the daemon shows the test
        // while it runs, and not after it ended.
        let line = format!(
            "real-session test of {TAG} on {}, for 0 min\n",
            &commit[..8]
        );
        let shown = || async {
            let asked = remote::Request::Status;
            let wait = Duration::from_secs(30);
            remote::ask(&r.api(&r.lead), &r.lead, host, &asked, wait)
                .await
                .unwrap()
        };
        let status = shown().await;
        assert!(status.contains(&line), "{status}");
        std::fs::remove_file(&hold).unwrap();
        let want = format!("state={state}");
        until("the final status", || async {
            std::fs::read_to_string(&log)
                .ok()
                .filter(|l| l.contains(&want))
        })
        .await;
        until("the end of the test in the status", || async {
            (!shown().await.contains("real-session test")).then_some(())
        })
        .await;
        // The daemon wakes the lead at the end, one time, with the result.
        let word = if state == "success" {
            "passed"
        } else {
            "failed"
        };
        let start = format!("The real-session test of {TAG} {word} on {commit}.");
        until("the message to the lead", || async {
            (!r.messages(&start).await.is_empty()).then_some(())
        })
        .await;
        tokio::time::sleep(Duration::from_millis(LOOK_MS * 10)).await;
        let told = r.messages(&start).await;
        assert_eq!(told.len(), 1, "{told:#?}");
        assert!(told[0].contains("next: "), "{told:#?}");
        let left = riff::watches::Watches::new(&dir).read().unwrap();
        assert!(left.is_empty(), "the watch goes: {left:?}");
        // #1597: the calls of `gh` of the test are in the count of the daemon.
        let status = shown().await;
        assert!(
            status
                .lines()
                .any(|l| l.trim_start().starts_with("release test ")),
            "{status}"
        );
        let seen = std::fs::read_to_string(&log).unwrap();
        assert!(seen.contains("state=pending"), "{seen}");
        assert!(seen.contains(&format!("statuses/{commit}")), "{seen}");
        assert!(seen.contains("context=riff/real-session"), "{seen}");
        let comment = std::fs::read_to_string(tmp.path().join("comment")).unwrap_or_default();
        if state == "failure" {
            // #1255: the reason is on the release issue and in the status.
            assert!(seen.contains("issue comment 7"), "{seen}");
            assert!(seen.contains("description=FAIL at step 6 of 10"), "{seen}");
            assert!(
                comment.contains("Failed step: step 6 of 10, run the item: exit 1"),
                "{comment}"
            );
            assert!(comment.contains("a line"), "{comment}");
            assert!(comment.contains("GH_TOKEN=[secret]"), "{comment}");
            assert!(!comment.contains("ghp_secret1"), "{comment}");
            std::fs::remove_file(tmp.path().join("comment")).unwrap();
        } else {
            assert!(comment.is_empty(), "{comment}");
        }
        let server = tmp.path().join("server");
        assert_eq!(std::fs::read_to_string(&server).unwrap(), &*r.server);
        std::fs::remove_file(&log).unwrap();
        std::fs::remove_file(&server).unwrap();
    }

    // A work session gets the refusal, and the test does not start.
    let worker = SessionUri::new(
        Who::new("mike", Some("w2")).unwrap(),
        r.lead.place().clone(),
    );
    r.api(&worker).register(&worker).await.unwrap();
    let asked = remote::Request::ReleaseTest {
        commit,
        tag: TAG.to_owned(),
    };
    let err = remote::ask(
        &r.api(&worker),
        &worker,
        "b",
        &asked,
        Duration::from_secs(30),
    )
    .await
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("which is not the lead of mike in"),
        "{err:#}"
    );
    let seen = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!seen.contains("real-session"), "{seen}");
}

/// `git ARGS` in `dir`, with an identity. Gives its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    text(&out.stdout).trim().to_owned()
}

/// Commits `config` as the `riff.toml` of the clone `dir`. Gives the commit.
fn commit_config(dir: &Path, config: &str) -> String {
    std::fs::write(dir.join("riff.toml"), config).unwrap();
    git(dir, &["commit", "-q", "-a", "-m", "config"]);
    git(dir, &["rev-parse", "HEAD"])
}

/// Gives the main clone `main` an `origin`, a bare clone of it in
/// `tmp/origin.git`, with its `origin/HEAD`. Gives the bare clone.
fn add_origin(main: &Path, tmp: &Path) -> PathBuf {
    git(
        tmp,
        &[
            "clone",
            "-q",
            "--bare",
            &main.display().to_string(),
            "origin.git",
        ],
    );
    let bare = tmp.join("origin.git");
    git(
        main,
        &["remote", "add", "origin", &bare.display().to_string()],
    );
    git(main, &["fetch", "-q", "origin"]);
    git(main, &["remote", "set-head", "origin", "--auto"]);
    bare
}

/// A `just` that makes the riff of the build: the commit that it builds.
const FAKE_JUST: &str =
    "#!/bin/sh\nmkdir -p target/static/release\ngit rev-parse HEAD > target/static/release/riff\n";

/// As [`fake_real_session`], with a fake `just` for `riff build` and the
/// check clone in `dir/check`. Gives the variables for a host.
fn fake_build(dir: &Path, log: &Path, code: &Path) -> Vec<(String, String)> {
    let mut extra = fake_real_session(dir, log, code);
    isolated::write_program(dir.join("bin/just"), FAKE_JUST);
    extra.push((
        riff::check::VAR.into(),
        dir.join("check").display().to_string(),
    ));
    extra
}

/// 01M5SMPX8NM75P8E38DGQ17W7B, 01M5TKEH5E2ZQ22PAN9FTT27VW: a repository
/// with no `[release]` in the merged `riff.toml` has no real-session test
/// and no `riff build` on the daemon: both refuse with one line, and the
/// daemon runs no program and calls no `gh`. The `[release]` of the
/// working tree of the main clone does not count.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_refuses_the_test_and_the_build_of_a_repository_with_no_release_table() {
    let tmp = tempfile::tempdir().unwrap();
    let (log, code) = (tmp.path().join("gh.log"), tmp.path().join("code"));
    let extra = fake_build(tmp.path(), &log, &code);
    let r = riff_env(300, 300, &extra).await;
    std::fs::write(&code, "0").unwrap();
    // The default branch of `origin` has no `[release]`. The working tree of
    // the main clone has one.
    let commit = commit_config(&r.main, "[session]\nimage = \"x\"\n");
    add_origin(&r.main, tmp.path());
    std::fs::write(r.main.join("riff.toml"), "[release]\n").unwrap();

    let test = remote::Request::ReleaseTest {
        commit,
        tag: TAG.to_owned(),
    };
    for asked in [test, remote::Request::Build] {
        let err = remote::ask(
            &r.api(&r.lead),
            &r.lead,
            "a",
            &asked,
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err:#}").contains(riff::text::NO_RELEASE),
            "{asked}: {err:#}"
        );
    }
    // The daemon asks `gh` for its own token: it sets no status and makes no
    // comment.
    let seen = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!seen.contains("statuses"), "{seen}");
    assert!(!seen.contains("issue"), "{seen}");
    assert!(!tmp.path().join("server").exists(), "the test ran");
    assert!(!tmp.path().join("check").exists(), "the build ran");
}

/// 01M5TKEH5E2ZQ22PAN9FTT27VW: the daemon reads the `[release]` of the
/// default branch of `origin`, after a fetch. The main clone is older than
/// the merge of `[release]`: its working tree has none. The real-session
/// test and the build run all the same, and the daemon moves the working
/// tree of the main clone not at all.
#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_runs_the_test_and_the_build_with_the_release_table_that_origin_has() {
    let tmp = tempfile::tempdir().unwrap();
    let (log, code) = (tmp.path().join("gh.log"), tmp.path().join("code"));
    let extra = fake_build(tmp.path(), &log, &code);
    let r = riff_env(300, 300, &extra).await;
    std::fs::write(&code, "0").unwrap();
    let old = "[session]\nimage = \"x\"\n";
    commit_config(&r.main, old);
    let bare = add_origin(&r.main, tmp.path());
    // Another clone merges `[release]`. The main clone has not fetched it.
    let other = tmp.path().join("other");
    git(
        tmp.path(),
        &["clone", "-q", &bare.display().to_string(), "other"],
    );
    let merged = commit_config(&other, "[release]\n");
    git(&other, &["push", "-q", "origin", "HEAD"]);

    let asked = remote::Request::ReleaseTest {
        commit: merged.clone(),
        tag: TAG.to_owned(),
    };
    let wait = Duration::from_secs(30);
    let reply = remote::ask(&r.api(&r.lead), &r.lead, "a", &asked, wait)
        .await
        .unwrap();
    assert!(reply.contains("started on"), "{reply}");
    until("the final status", || async {
        std::fs::read_to_string(&log)
            .ok()
            .filter(|l| l.contains("state=success"))
    })
    .await;

    let built = remote::ask(&r.api(&r.lead), &r.lead, "a", &remote::Request::Build, wait)
        .await
        .unwrap();
    let riff = tmp.path().join("check/target/static/release/riff");
    assert_eq!(built, riff.display().to_string());
    assert_eq!(std::fs::read_to_string(&riff).unwrap().trim(), merged);
    assert_eq!(
        std::fs::read_to_string(r.main.join("riff.toml")).unwrap(),
        old
    );
}

/// #1262: a daemon that runs the wave of a repository serves `--host` for
/// it at its start, with no `riff join`: it writes `served.json`
/// from `waves.json`.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_with_a_wave_and_no_served_file_serves_host() {
    let server = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    let api = crate::common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    crate::common::name_lead(&server.api("mike", None), &lead).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let mut a = Host::new("a", &server);
    a.claude_ends_in(300);
    std::fs::create_dir_all(a.dir()).unwrap();
    let repo = lead.place().repo_text();
    let waves = serde_json::json!({repo.clone(): {
        "repo": repo, "main": main, "server": &*server, "host": "a",
    }});
    std::fs::write(a.dir().join("waves.json"), waves.to_string()).unwrap();
    assert!(!a.dir().join("served.json").exists());
    a.start().await;

    let out = until("the status of the host", || async {
        let out = a
            .run(&main, &["daemon", "status", "--host", "a"], Some("l1"))
            .await;
        out.status.success().then_some(out)
    })
    .await;
    assert!(text(&out.stdout).contains("Its folder:"), "{out:?}");
    let served = remote::read_served(&a.dir()).unwrap();
    assert_eq!(served[&repo].wave.server, &*server);
}

/// 01M5R56GN3DC09GAPAF69GENEZ (#1404): `waves.json` and `served.json` of
/// an older build have no default branch. The daemon of a later build
/// reads both, serves the repository, and keeps the session that the older
/// build gave it.
#[tokio::test(flavor = "multi_thread")]
async fn a_daemon_with_the_served_file_of_an_older_build_serves_host() {
    let server = start_server().await;
    let root = tempfile::tempdir().unwrap();
    let main = repository(root.path());
    let lead = session(&main, "mike", "a", "l1");
    let api = crate::common::api_of(&server, &lead);
    api.register(&lead).await.unwrap();
    crate::common::name_lead(&server.api("mike", None), &lead).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    let mut a = Host::new("a", &server);
    a.claude_ends_in(300);
    std::fs::create_dir_all(a.dir()).unwrap();
    let repo = lead.place().repo_text();
    let wave = serde_json::json!({
        "repo": repo, "main": main, "server": &*server, "host": "a",
    });
    let waves = serde_json::json!({repo.clone(): wave.clone()});
    std::fs::write(a.dir().join("waves.json"), waves.to_string()).unwrap();
    let session = riff::identity::new_session_id();
    let served = serde_json::json!({repo.clone(): {"wave": wave, "session": session}});
    std::fs::write(a.dir().join("served.json"), served.to_string()).unwrap();
    a.start().await;

    let out = until("the status of the host", || async {
        let out = a
            .run(&main, &["daemon", "status", "--host", "a"], Some("l1"))
            .await;
        out.status.success().then_some(out)
    })
    .await;
    assert!(text(&out.stdout).contains("Its folder:"), "{out:?}");
    let kept = remote::read_served(&a.dir()).unwrap();
    assert_eq!(kept[&repo].session, session);
    assert_eq!(kept[&repo].wave.server, &*server);
    let written = std::fs::read_to_string(a.dir().join("served.json")).unwrap();
    assert!(!written.contains("branch"), "{written}");
}

/// The repository `name` in `origins`, with the pin of the session image
/// that a wave needs. Its folder name makes its repository `local/NAME`.
fn origin(origins: &Path, name: &str) -> PathBuf {
    let dir = origins.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    let pin = format!(
        "[session]\nimage = \"ghcr.io/o/i@sha256:{}\"\n",
        "0".repeat(64)
    );
    std::fs::write(dir.join("riff.toml"), pin).unwrap();
    git(&["add", "riff.toml"]);
    git(&["commit", "-q", "-m", "pin"]);
    std::fs::canonicalize(&dir).unwrap()
}

/// A fake forge for the hosts (#1430): a `gh` that logs each call in
/// `tmp/gh.log`, makes the clone `gh repo clone local/NAME PATH` from
/// `tmp/origins/NAME`, and fails each other call, and a `podman` that has
/// each image. Gives the variables for a host.
fn fake_forge(tmp: &Path) -> Vec<(String, String)> {
    let bin = tmp.join("forge-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let gh = format!(
        "#!/bin/sh\necho \"$*\" >> {log}\n\
         if [ \"$1 $2\" = \"repo clone\" ]; then exec git clone -q {origins}/\"${{3#local/}}\" \"$4\"; fi\n\
         exit 1\n",
        log = tmp.join("gh.log").display(),
        origins = tmp.join("origins").display(),
    );
    isolated::write_program(bin.join("gh"), gh);
    isolated::write_program(bin.join("podman"), "#!/bin/sh\nexit 0\n");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    vec![("PATH".into(), path)]
}

/// The lines of `tmp/gh.log` that clone a repository.
fn clones(tmp: &Path) -> Vec<String> {
    std::fs::read_to_string(tmp.join("gh.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("repo clone "))
        .map(str::to_owned)
        .collect()
}

/// #1430, 01M5SN561BM9SK24SS0AVJ3P5J, 01M5SN5644YVEPF3D452ZA2JVD:
/// `riff join --host b` for a repository that `b` does not serve.
/// The daemon of `b` makes the main clone with the `gh` of the user,
/// serves the repository and runs its wave. A second call changes
/// nothing: no new clone, the same session, the same files.
#[tokio::test(flavor = "multi_thread")]
async fn wave_run_with_host_makes_the_main_clone_on_a_host_that_does_not_serve_the_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let other = origin(&tmp.path().join("origins"), "other");
    let r = riff_env(300, 300, &fake_forge(tmp.path())).await;
    // The lead of local/other runs on `a`. Only local/riff is served on `b`.
    let lead = session(&other, "mike", "a", "l2");
    r.api(&lead).register(&lead).await.unwrap();
    crate::common::name_lead(&r.server.api("mike", None), &lead).await;
    r.api(&lead)
        .set_riff(&lead, RiffState::Running)
        .await
        .unwrap();
    let repo = lead.place().repo_text();
    assert_eq!(repo, "local/other");
    assert!(!remote::read_served(&r.b.dir()).unwrap().contains_key(&repo));
    let clone = r.b.dir().join("clones/local/other");
    assert!(!clone.exists());

    let out = r.a.run(&other, &["join", "--host", "b"], Some("l2")).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}\n{}", r.b.output());
    assert!(
        stdout.contains(&riff::text::daemon_cloned(&repo, "b", &clone)),
        "{stdout}"
    );
    assert!(stdout.contains(&riff::text::wave_asked(&repo)), "{stdout}");

    // The clone is the origin, the daemon serves it, and its wave runs.
    assert!(clone.join(".git").exists() && clone.join("riff.toml").exists());
    assert_eq!(clones(tmp.path()).len(), 1, "{:?}", clones(tmp.path()));
    let served = remote::read_served(&r.b.dir()).unwrap();
    assert_eq!(served[&repo].wave.main, clone);
    assert_eq!(served[&repo].wave.host, "b");
    let waves = riff::wave::read_waves(&r.b.dir()).unwrap();
    assert_eq!(waves[&repo].main, clone);
    // The daemon of `a` serves nothing new.
    assert!(!remote::read_served(&r.a.dir()).unwrap().contains_key(&repo));

    // The daemon of `b` has a session for the repository now: the
    // lead reaches it with the other commands too.
    let id = until("the daemon of the new repository in who", || async {
        let who = r.api(&lead).who(&lead, false).await.ok()?;
        remote::find(&who, "mike", &repo, "b").map(str::to_owned)
    })
    .await;
    assert_eq!(id, served[&repo].session);
    let out =
        r.a.run(&other, &["daemon", "status", "--host", "b"], Some("l2"))
            .await;
    assert!(out.status.success(), "{out:?}");
    assert!(text(&out.stdout).contains(&repo), "{out:?}");

    // A second call changes nothing.
    let files = |host: &Host| {
        ["served.json", "waves.json", "parts.json"]
            .map(|f| std::fs::read_to_string(host.dir().join(f)).unwrap())
    };
    let before = files(&r.b);
    let head = |dir: &Path| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        text(&out.stdout)
    };
    let (commit, mtime) = (
        head(&clone),
        std::fs::metadata(&clone).unwrap().modified().unwrap(),
    );
    let out = r.a.run(&other, &["join", "--host", "b"], Some("l2")).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}\n{}", r.b.output());
    assert!(!stdout.contains("cloned"), "{stdout}");
    assert!(stdout.contains(&riff::text::wave_asked(&repo)), "{stdout}");
    assert_eq!(files(&r.b), before);
    assert_eq!(clones(tmp.path()).len(), 1, "{:?}", clones(tmp.path()));
    assert_eq!(head(&clone), commit);
    assert_eq!(
        std::fs::metadata(&clone).unwrap().modified().unwrap(),
        mtime
    );
    assert_eq!(
        std::fs::read_dir(r.b.dir().join("clones/local"))
            .unwrap()
            .count(),
        1
    );

    // The person asks for it from a terminal too, for a host that is served.
    let out = r.a.run(&other, &["join", "--host", "b"], None).await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(clones(tmp.path()).len(), 1);
}

/// The repository `local/other` for two daemons that run its wave (#1439):
/// a `gh` that clones it, and that answers from the files `milestones`,
/// `issues` and `pulls` in `tmp/forge`, and a `podman` that has each image.
/// It logs `HOST CALL` for each call in `tmp/gh.log`. The wave 7 has the
/// issue 1, with no `Done when:` line, and the issue 2, whose pull request
/// has a passed verify and auto-merge off, also when the wave asks GitHub
/// again ([`riff::rollout::merge_off`]). Gives the variables for a host.
fn fake_wave_forge(tmp: &Path) -> Vec<(String, String)> {
    let bin = tmp.join("forge-bin");
    let forge = tmp.join("forge");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&forge).unwrap();
    let issue = |n: u64, body: &str| {
        format!(
            r#"{{"number":{n},"title":"t{n}","body":"{body}","comments":[],"milestone":{{"title":"Wave 7"}},"labels":[]}}"#
        )
    };
    let pull = r#"{"number":42,"headRefName":"worktree-issue-2","headRefOid":"1a2b3c4","isDraft":false,"statusCheckRollup":[{"__typename":"StatusContext","context":"riff/verify","state":"SUCCESS"}],"mergeable":"MERGEABLE","autoMergeRequest":null,"body":"Closes #2\n\nIssue: #2\nMilestone: Wave 7\n"}"#;
    let files = [
        (
            "milestones",
            r#"[{"number":7,"title":"Wave 7","open_issues":2,"closed_issues":0}]"#.to_owned(),
        ),
        (
            "issues",
            format!("[{},{}]", issue(1, "No line."), issue(2, "Done when: x.")),
        ),
        ("pulls", format!("[{pull}]")),
    ];
    for (name, text) in files {
        std::fs::write(forge.join(name), text).unwrap();
    }
    let gh = format!(
        "#!/bin/sh\necho \"$RIFF_HOST $*\" >> {log}\n\
         case \"$1 $2\" in\n\
         \"repo clone\") exec git clone -q {origins}/\"${{3#local/}}\" \"$4\" ;;\n\
         api*)\n\
           case \"$2\" in\n\
             graphql) case \"$*\" in *'pullRequest(number'*) echo '{{\"data\":{{\"repository\":{{\"pullRequest\":{{\"state\":\"OPEN\",\"isInMergeQueue\":false,\"headRefOid\":\"1a2b3c4\",\"autoMergeRequest\":null}}}}}}}}' ;; *) exit 1 ;; esac ;;\n\
             repos/local/other) case \"$*\" in *--jq*) echo main ;; *) echo '{{\"default_branch\":\"main\"}}' ;; esac ;;\n\
             */rules/branches/main*) echo '[]' ;;\n\
             */milestones*) cat {forge}/milestones ;;\n\
             */check-runs*) echo '{{\"check_runs\":[{{\"status\":\"completed\",\"conclusion\":\"success\",\"head_sha\":\"x\"}}]}}' ;;\n\
             *) exit 1 ;;\n\
           esac ;;\n\
         \"pr list\") cat {forge}/pulls ;;\n\
         \"issue list\")\n\
           case \"$*\" in\n\
             *--milestone*) if [ -e {forge}/fault ]; then echo \"GraphQL: Something went wrong while executing your query for user ID 1.\" >&2; exit 1; fi ;;\n\
           esac\n\
           cat {forge}/issues ;;\n\
         *) exit 1 ;;\n\
         esac\n",
        log = tmp.join("gh.log").display(),
        origins = tmp.join("origins").display(),
        forge = forge.display(),
    );
    isolated::write_program(bin.join("gh"), gh);
    isolated::write_program(bin.join("podman"), "#!/bin/sh\nexit 0\n");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    vec![("PATH".into(), path)]
}

/// The lines of `tmp/gh.log` of `host` that name `call`.
fn gh_calls(tmp: &Path, host: &str, call: &str) -> usize {
    std::fs::read_to_string(tmp.join("gh.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with(&format!("{host} ")) && l.contains(call))
        .count()
}

/// Each post of the person to the lead `lead`, since the last call.
async fn person_posts(api: &Api, lead: &SessionUri) -> Vec<String> {
    api.inbox(lead, None, false)
        .await
        .unwrap()
        .into_iter()
        .flat_map(|i| i.messages)
        .map(|c| c.message)
        .filter(|m| m.from.who().session().is_none())
        .map(|m| m.body)
        .collect()
}

/// #1439, 01M6FZY6NABXGXVRYF6W1509XE: the wave of the repository runs on
/// two hosts, with a daemon on each, on one riff-server. Both daemons
/// look at an issue with no `Done when:` line, a pull request that is
/// stuck, and the wave that is complete. The lead gets each of the three
/// messages one time, not one time for each host.
#[tokio::test(flavor = "multi_thread")]
async fn two_daemons_of_one_wave_give_the_lead_each_notice_one_time() {
    let tmp = tempfile::tempdir().unwrap();
    let other = origin(&tmp.path().join("origins"), "other");
    let r = riff_env(300, 300, &fake_wave_forge(tmp.path())).await;
    let lead = session(&other, "mike", "a", "l2");
    r.api(&lead).register(&lead).await.unwrap();
    crate::common::name_lead(&r.server.api("mike", None), &lead).await;
    r.api(&lead)
        .set_riff(&lead, RiffState::Running)
        .await
        .unwrap();
    let api = r.api(&lead);
    // The lead has no unread message before the wave.
    person_posts(&api, &lead).await;

    for host in ["a", "b"] {
        let out = r.a.run(&other, &["join", "--host", host], Some("l2")).await;
        assert!(out.status.success(), "{out:?}\n{}", r.host(host).output());
    }

    // Both daemons look at the wave, and the watch of each one reads it.
    let both = |call: &str, n: usize| {
        let (a, b) = (
            gh_calls(tmp.path(), "a", call),
            gh_calls(tmp.path(), "b", call),
        );
        (a >= n && b >= n).then_some(())
    };
    let milestone = "--milestone";
    until("both daemons look at the wave", || async {
        both("issue list", 1)
            .and(both(milestone, 1))
            .and(both("pr list", 1))
    })
    .await;
    // The first post of a daemon has gone out when the second one posts.
    let told = std::sync::Mutex::new(Vec::new());
    until("the lead hears of both", || async {
        let posts = person_posts(&api, &lead).await;
        let mut told = told.lock().unwrap();
        told.extend(posts);
        (told.len() >= 2).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut told = told.into_inner().unwrap();
    told.extend(person_posts(&api, &lead).await);
    told.sort();
    let mut expected = vec![
        riff::text::wave_no_done_when(1),
        riff::text::wave_stuck(
            "local/other",
            42,
            "worktree-issue-2",
            "main",
            "1a2b3c4",
            &riff::rollout::Stuck::AutoMergeOff,
        ),
    ];
    expected.sort();
    assert_eq!(told, expected, "one message for each, from two daemons");

    // The last item closes: both watches see the wave complete.
    let seen = [
        gh_calls(tmp.path(), "a", milestone),
        gh_calls(tmp.path(), "b", milestone),
    ];
    let forge = tmp.path().join("forge");
    std::fs::write(forge.join("issues"), "[]").unwrap();
    std::fs::write(forge.join("pulls"), "[]").unwrap();
    until("both watches look at the closed wave", || async {
        let (a, b) = (
            gh_calls(tmp.path(), "a", milestone),
            gh_calls(tmp.path(), "b", milestone),
        );
        (a >= seen[0] + 3 && b >= seen[1] + 3).then_some(())
    })
    .await;
    let told = std::sync::Mutex::new(Vec::new());
    until("the lead hears that the wave is complete", || async {
        let posts = person_posts(&api, &lead).await;
        let mut told = told.lock().unwrap();
        told.extend(posts);
        (!told.is_empty()).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut told = told.into_inner().unwrap();
    told.extend(person_posts(&api, &lead).await);
    assert_eq!(told, ["Wave 7 is complete: next riff wave end 7"]);
}

/// #1550, 01M6H160E2ZK2AM0PCTQG82V2G: the wave runs on two hosts, and
/// GraphQL fails for the token: `gh issue list` of the watch of the wave
/// fails on both hosts, at each look. The lead gets the fault one time,
/// with one dot at the end of the error. The fault is not a limit of
/// GitHub: a limit makes the watches wait (#1557).
#[tokio::test(flavor = "multi_thread")]
async fn two_daemons_that_cannot_read_the_wave_give_the_lead_the_fault_one_time() {
    let tmp = tempfile::tempdir().unwrap();
    let other = origin(&tmp.path().join("origins"), "other");
    let env = fake_wave_forge(tmp.path());
    std::fs::write(tmp.path().join("forge").join("fault"), "").unwrap();
    let r = riff_env(300, 300, &env).await;
    let lead = session(&other, "mike", "a", "l2");
    r.api(&lead).register(&lead).await.unwrap();
    crate::common::name_lead(&r.server.api("mike", None), &lead).await;
    r.api(&lead)
        .set_riff(&lead, RiffState::Running)
        .await
        .unwrap();
    let api = r.api(&lead);
    person_posts(&api, &lead).await;

    for host in ["a", "b"] {
        let out = r.a.run(&other, &["join", "--host", host], Some("l2")).await;
        assert!(out.status.success(), "{out:?}\n{}", r.host(host).output());
    }

    // Each watch fails at its first look, and again at its second look.
    until("both watches fail two times", || async {
        let fails = |host| gh_calls(tmp.path(), host, "--milestone") >= 2;
        (fails("a") && fails("b")).then_some(())
    })
    .await;
    let told = std::sync::Mutex::new(Vec::new());
    until("the lead hears the fault", || async {
        let posts = person_posts(&api, &lead).await;
        let mut told = told.lock().unwrap();
        told.extend(posts);
        told.iter()
            .any(|m: &String| m.contains("cannot read the state"))
            .then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut told = told.into_inner().unwrap();
    told.extend(person_posts(&api, &lead).await);
    let faults: Vec<_> = told
        .iter()
        .filter(|m| m.contains("cannot read the state"))
        .collect();
    assert_eq!(faults.len(), 1, "one message, from two daemons: {told:?}");
    assert!(
        faults[0].starts_with("riff cannot read the state of the current wave: "),
        "{faults:?}"
    );
    assert!(
        faults[0].contains("user ID 1. riff tries again each minute"),
        "one dot: {faults:?}"
    );
}

/// #1643: `riff join --host b` gives `b` the part workers, and `riff
/// leave --host b` takes them. A host with no other part stops serving the
/// repository and removes the main clone that its daemon made. The leave
/// holds over a restart of the daemon. A host with no daemon of the user
/// fails with the words of riff.
#[tokio::test(flavor = "multi_thread")]
async fn leave_with_host_stops_serving_and_removes_the_clone_that_the_daemon_made() {
    let tmp = tempfile::tempdir().unwrap();
    let third = origin(&tmp.path().join("origins"), "third");
    let mut r = riff_env(300, 300, &fake_forge(tmp.path())).await;
    let repo = "local/third";
    let clone = r.b.dir().join("clones/local/third");

    // The person asks from a terminal in the clone.
    let out = r.a.run(&third, &["join", "--host", "b"], None).await;
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{out:?}\n{}", r.b.output());
    assert!(
        stdout.contains(&riff::text::daemon_cloned(repo, "b", &clone)),
        "{stdout}"
    );
    assert!(clone.join("riff.toml").exists());
    assert!(remote::read_served(&r.b.dir()).unwrap().contains_key(repo));
    assert!(
        riff::daemon::parts::of(&r.b.dir(), repo).workers,
        "the host has the part workers"
    );
    assert_eq!(clones(tmp.path()).len(), 1);
    // `riff who` shows the parts of the host.
    let who = until("the parts of b in riff who", || async {
        let out = r.a.run(&r.main, &["who"], Some("l1")).await;
        let who = text(&out.stdout);
        who.contains("local/third workers").then_some(who)
    })
    .await;
    assert!(
        who.contains("parts local/riff lead, local/third workers"),
        "{who}"
    );
    // The leave goes to the session of the daemon for the repository: the
    // daemon starts it after the join.
    let lead = session(&third, "mike", "a", "l3");
    r.api(&lead).register(&lead).await.unwrap();
    until("the daemon of b in who of the repository", || async {
        let who = r.api(&lead).who(&lead, false).await.ok()?;
        remote::find(&who, "mike", repo, "b").map(str::to_owned)
    })
    .await;

    // The leave takes the part workers, and the host has no other part.
    let out = r.a.run(&third, &["leave", "--host", "b"], None).await;
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}\n{}", r.b.output());
    assert!(stdout.contains(&riff::text::leave_stops(repo)), "{stdout}");
    assert!(stdout.contains(&riff::text::retire_stops(repo)), "{stdout}");
    assert!(
        stdout.contains(&riff::text::retire_removes(repo, &clone)),
        "{stdout}"
    );
    assert!(!clone.exists());
    assert!(!remote::read_served(&r.b.dir()).unwrap().contains_key(repo));
    assert!(riff::wave::read_waves(&r.b.dir()).unwrap().is_empty());
    assert!(riff::daemon::parts::of(&r.b.dir(), repo).none());
    // Its session on riff-server ends.
    until(
        "the daemon of b ends its session for the repository",
        || async {
            let who = r.api(&lead).who(&lead, false).await.ok()?;
            remote::find(&who, "mike", repo, "b")
                .is_none()
                .then_some(())
        },
    )
    .await;

    // A second leave: the host has nothing to leave.
    let out = r.a.run(&third, &["leave", "--host", "b"], None).await;
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(
        text(&out.stderr).trim(),
        riff::text::no_daemon_leave("b", repo)
    );

    // The leave holds over a restart of the daemon.
    r.b.stop();
    r.b.start().await;
    tokio::time::sleep(Duration::from_millis(LOOK_MS * 5)).await;
    assert!(!remote::read_served(&r.b.dir()).unwrap().contains_key(repo));
    assert!(riff::wave::read_waves(&r.b.dir()).unwrap().is_empty());
    assert!(riff::daemon::parts::of(&r.b.dir(), repo).none());
    let status = r.lead(&["daemon", "status"], "b").await;
    assert!(!text(&status.stdout).contains(repo), "{status:?}");

    // A host with no daemon of the user.
    let out = r.a.run(&third, &["join", "--host", "c"], None).await;
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(
        text(&out.stderr).trim(),
        riff::text::no_daemon_host("c", repo)
    );
    assert_eq!(clones(tmp.path()).len(), 1);
}

/// A `gh` whose CI run of `TAG` has the state in the file `dir/state`:
/// `waiting` (the job `Deploy` waits for a person), `running`, `success`
/// or `failure`. The list of the release issues is the file `dir/issues`,
/// or empty. Every other call answers with an empty list. Gives the
/// variables for a host.
fn fake_deploy(dir: &Path) -> Vec<(String, String)> {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let url = "https://github.com/o/r/actions/runs/9";
    let body = format!(
        "#!/bin/sh\nstate=$(cat {state} 2>/dev/null)\n\
         case \"$state\" in\n\
           waiting) status=waiting; job=waiting; conclusion= ;;\n\
           running) status=in_progress; job=in_progress; conclusion= ;;\n\
           *) status=completed; job=completed; conclusion=$state ;;\n\
         esac\n\
         case \"$*\" in\n\
         'run list'*)\n\
           printf '[{{\"databaseId\":9,\"headBranch\":\"{TAG}\",\"headSha\":\"abc\",\"url\":\"{url}\",\"displayTitle\":\"Release\",\"status\":\"%s\",\"conclusion\":\"%s\"}}]\\n' \"$status\" \"$conclusion\" ;;\n\
         'run view 9 --json jobs')\n\
           printf '{{\"jobs\":[{{\"name\":\"Deploy\",\"status\":\"%s\",\"conclusion\":\"%s\",\"startedAt\":\"\"}}]}}\\n' \"$job\" \"$conclusion\" ;;\n\
         'issue list'*) cat {issues} 2>/dev/null || echo '[]' ;;\n\
         *) echo '[]' ;;\n\
         esac\n",
        state = dir.join("state").display(),
        issues = dir.join("issues").display(),
    );
    isolated::write_program(bin.join("gh"), body);
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    vec![("PATH".into(), path)]
}

/// 01M5SNV75DRFHR26BT2M8243VV: the watch of the daemon gives the lead of
/// another repository one note when the job `Deploy` waits, one when it
/// passes and one when it fails, with a real riff-server. The lead of the
/// release gets its own message, and none of the notes.
#[tokio::test(flavor = "multi_thread")]
async fn the_watch_of_a_deploy_gives_the_other_lead_one_note_for_each_step() {
    let tmp = tempfile::tempdir().unwrap();
    let extra = fake_deploy(tmp.path());
    let r = riff_env(300, 300, &extra).await;
    // The watch fetches the main clone: it needs an origin.
    let bare = tmp.path().join("origin.git");
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(
        tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &r.main.display().to_string(),
            "origin.git",
        ],
    );
    git(
        &r.main,
        &["remote", "add", "origin", &bare.display().to_string()],
    );
    // The second lead of the riff: bob, in another repository.
    let other: SessionUri = "riff://bob@b/n8behavior/sens-o-matic?session=b1"
        .parse()
        .unwrap();
    r.api(&other).register(&other).await.unwrap();
    crate::common::name_lead(&r.server.api("bob", None), &other).await;

    let repo = r.lead.place().repo_text();
    let dir = r.host("a").dir();
    let state = tmp.path().join("state");
    let lead = |word: &str| format!("The deploy of {TAG} {word}");
    let heard = || async {
        let api = r.api(&other);
        match api.read(&other, &riff::hold::thread(), true).await {
            Ok(read) => read.into_iter().map(|m| m.message.body).collect(),
            Err(_) => Vec::new(),
        }
    };
    let expect =
        |step: riff::hold::Step| riff::text::release_note(step, TAG, &riff::hold::by(&repo));
    let watch = || {
        let op = riff::watches::Op::Deploy {
            tag: TAG.to_owned(),
            stale: None,
        };
        riff::watches::Watches::new(&dir)
            .register(&repo, op, riff::daemon::now_ms())
            .unwrap();
    };

    // The job waits: the lead and the other lead hear it once.
    std::fs::write(&state, "waiting").unwrap();
    watch();
    until("the note of the wait", || async {
        (heard().await.len() == 1).then_some(())
    })
    .await;
    assert_eq!(heard().await, [expect(riff::hold::Step::Waits)]);
    until("the message to the lead", || async {
        (!r.messages(&lead("waits for a person")).await.is_empty()).then_some(())
    })
    .await;

    // The job passes: one more note, and the watch goes.
    std::fs::write(&state, "success").unwrap();
    until("the note of the pass", || async {
        (heard().await.len() == 2).then_some(())
    })
    .await;
    assert_eq!(
        heard().await,
        [
            expect(riff::hold::Step::Waits),
            expect(riff::hold::Step::Deployed)
        ]
    );
    until("the message of the pass", || async {
        (!r.messages(&lead("passed")).await.is_empty()).then_some(())
    })
    .await;

    // Another deploy fails: one more note.
    std::fs::write(&state, "failure").unwrap();
    watch();
    until("the note of the failure", || async {
        (heard().await.len() == 3).then_some(())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(LOOK_MS * 10)).await;
    assert_eq!(
        heard().await,
        [
            expect(riff::hold::Step::Waits),
            expect(riff::hold::Step::Deployed),
            expect(riff::hold::Step::DeployFailed)
        ],
        "one note for each step"
    );
    assert_eq!(r.messages(&lead("passed")).await.len(), 1);
    assert_eq!(r.messages(&lead("failed")).await.len(), 1);
}

/// 01M6XTS7RGXB30MFM6DZ7PVZ75: `riff who` says the state of an open release
/// at each step, in the same line as the header of the TUI. The watch of the
/// deploy, then the watch of the check, give the step to the facts of the
/// host, with a real riff-server and a fake `gh`. With no open release, `riff
/// who` has no line `release`.
#[tokio::test(flavor = "multi_thread")]
async fn riff_who_says_the_state_of_an_open_release_at_each_step() {
    let tmp = tempfile::tempdir().unwrap();
    let extra = fake_deploy(tmp.path());
    let r = riff_env(300, 300, &extra).await;
    // The watch of the deploy fetches the main clone: it needs an origin.
    add_origin(&r.main, tmp.path());
    let repo = r.lead.place().repo_text();
    let dir = r.host("a").dir();
    let release = || async {
        let out = r.a.run(&r.main, &["who"], None).await;
        assert!(out.status.success(), "{out:?}");
        text(&out.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("release  "))
            .map(str::to_owned)
    };
    let shows = |want: Option<String>| async move {
        let what = format!("riff who: {want:?}");
        until(&what, || async { (release().await == want).then_some(()) }).await;
    };
    let set = |name: &str, text: &str| std::fs::write(tmp.path().join(name), text).unwrap();
    let lead = r.api(&r.lead);
    let issue = |open: bool, result: &str| {
        let body = format!("{}\\n\\n- a check: x", riff::cycle::check_mark(TAG, result));
        let state = if open { "OPEN" } else { "CLOSED" };
        format!(
            r#"[{{"number":7,"title":"Release {TAG}","state":"{state}","comments":[{{"body":"{body}"}}]}}]"#
        )
    };

    shows(None).await;
    set("state", "waiting");
    let op = riff::watches::Op::Deploy {
        tag: TAG.to_owned(),
        stale: None,
    };
    riff::watches::Watches::new(&dir)
        .register(&repo, op, riff::daemon::now_ms())
        .unwrap();
    shows(Some(format!(
        "{TAG}: the deploy waits for a person (riff release approve {TAG})"
    )))
    .await;

    // A person approves: the riff is paused, and the job runs.
    lead.set_riff(&r.lead, RiffState::Paused).await.unwrap();
    set("state", "running");
    shows(Some(format!("{TAG}: the deploy runs; the riff is paused"))).await;

    // The job passes: the watch of the check takes the place of the watch
    // of the deploy.
    set("state", "success");
    shows(Some(format!(
        "{TAG}: the deploy passed; it waits for the check of the lead of {repo}"
    )))
    .await;
    until("the watch of the check", || async {
        let watches = riff::watches::Watches::new(&dir).read().unwrap();
        watches
            .iter()
            .any(|w| matches!(w.op, riff::watches::Op::Check { .. }))
            .then_some(())
    })
    .await;

    // A check fails: the owner resumes, or the lead rolls back.
    set("issues", &issue(true, "failed"));
    shows(Some(format!(
        "{TAG}: the check failed: the owner resumes or rolls back"
    )))
    .await;

    // A check passes: riff closes the issue and resumes the riff.
    set("issues", &issue(false, "passed"));
    lead.set_riff(&r.lead, RiffState::Running).await.unwrap();
    shows(None).await;
    until("the end of the watch of the check", || async {
        let watches = riff::watches::Watches::new(&dir).read().unwrap();
        (!watches
            .iter()
            .any(|w| matches!(w.op, riff::watches::Op::Check { .. })))
        .then_some(())
    })
    .await;
}

/// A fake `gh` on the PATH for the watch of a wave: the open wave `Wave 41`
/// with no open item, the release issues of the wave from the file
/// `released`, the required check `Gate` of `main`, and its check runs from
/// the file `main`. The open release pull requests come from the file
/// `pulls`: #1140 has the head `abc`, with the check runs of the file `head`
/// and no status.
fn fake_wave(dir: &Path) -> Vec<(String, String)> {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let body = format!(
        "#!/bin/sh\n\
         case \"$*\" in\n\
         'api repos/{{owner}}/{{repo}}/milestones'*)\n\
           echo '[{{\"number\":41,\"title\":\"Wave 41\",\"open_issues\":0,\"closed_issues\":3}}]' ;;\n\
         'issue list --state all'*) cat {released} 2>/dev/null || echo '[]' ;;\n\
         'pr list --state open'*) cat {pulls} 2>/dev/null || echo '[]' ;;\n\
         'pr view 1140'*)\n\
           echo '{{\"number\":1140,\"title\":\"Release v9.9.9\",\"state\":\"OPEN\",\"headRefOid\":\"abc\"}}' ;;\n\
         'api repos/{{owner}}/{{repo}}/rules/'*)\n\
           echo '[{{\"type\":\"required_status_checks\",\"parameters\":{{\"required_status_checks\":[{{\"context\":\"Gate\"}}]}}}}]' ;;\n\
         'api repos/{{owner}}/{{repo}}/commits/main/check-runs'*) cat {main} ;;\n\
         'api repos/{{owner}}/{{repo}}/commits/abc/check-runs'*) cat {head} ;;\n\
         'api repos/{{owner}}/{{repo}}/commits/abc/status') echo '{{\"statuses\":[]}}' ;;\n\
         'api repos/{{owner}}/{{repo}}') echo '{{\"default_branch\":\"main\"}}' ;;\n\
         *) echo '[]' ;;\n\
         esac\n",
        released = dir.join("released").display(),
        main = dir.join("main").display(),
        pulls = dir.join("pulls").display(),
        head = dir.join("head").display(),
    );
    isolated::write_program(bin.join("gh"), body);
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    vec![("PATH".into(), path)]
}

/// 01M75N6YNHQQHGS0VGKTKSNJTR: `riff who` says that the current wave is
/// complete and that its release waits for its cut, and why, with a real
/// riff-server and a fake `gh`. The watch of the wave of the daemon gives the
/// wave to the facts of its host. 01M75R6FPVZA3JA5C48PQ26A85: when the wave
/// has its release issue, the line of the release at its step takes its
/// place.
#[tokio::test(flavor = "multi_thread")]
async fn riff_who_says_that_a_complete_wave_waits_for_its_cut() {
    let tmp = tempfile::tempdir().unwrap();
    let extra = fake_wave(tmp.path());
    let set = |name: &str, text: &str| std::fs::write(tmp.path().join(name), text).unwrap();
    let runs = |status: &str, conclusion: &str| {
        format!(
            r#"{{"check_runs":[{{"status":"{status}","conclusion":{conclusion},"head_sha":"abc"}}]}}"#
        )
    };
    set("main", &runs("completed", r#""failure""#));
    let r = riff_env(300, 300, &extra).await;
    // The read of a release asks `origin` for its tag.
    add_origin(&r.main, tmp.path());
    let repo = r.lead.place().repo_text();
    // The host a runs the wave of the repository: its daemon keeps the
    // watch of the wave.
    let wave = riff::wave::Wave {
        repo: repo.clone(),
        main: r.main.clone(),
        server: r.a.server.clone(),
        host: "a".into(),
    };
    let waves = std::collections::BTreeMap::from([(repo, wave)]);
    riff::wave::write_waves(&r.host("a").dir(), &waves).unwrap();
    let release = || async {
        let out = r.a.run(&r.main, &["who"], None).await;
        assert!(out.status.success(), "{out:?}");
        text(&out.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("release  "))
            .map(str::to_owned)
    };
    let shows = |want: Option<String>| async move {
        let what = format!("riff who: {want:?}");
        until(&what, || async { (release().await == want).then_some(()) }).await;
    };

    shows(Some(
        "Wave 41 is complete: the release waits for its cut; the full check of main failed".into(),
    ))
    .await;
    set("main", &runs("in_progress", "null"));
    shows(Some(
        "Wave 41 is complete: the release waits for its cut; the full check of main runs".into(),
    ))
    .await;
    // 01M7642H27T4862WZENFAN4277: the cut tags the last commit of main, and
    // the release issue of the wave is there: the line of the release at its
    // step takes the place of the line of the wave.
    git(&r.main, &["tag", "-a", "v9.9.9", "-m", "Release v9.9.9"]);
    git(&r.main, &["push", "-q", "origin", "v9.9.9"]);
    set(
        "released",
        r#"[{"number":7,"title":"Release v9.9.9","state":"OPEN"}]"#,
    );
    shows(Some("v9.9.9: the CI run of the tag builds".into())).await;
}
