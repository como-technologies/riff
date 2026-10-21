//! `riff top`: a live table of each session, its item and its status
//! (01M3NB54P1RBHTA5TKXP8BMY3K). It makes only read calls
//! (01M3NB589WMPRSAR43BSG9SP41).

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use riff_client::api::Api;
use riff_core::name::SessionUri;
use riff_core::wire::{RiffBinary, StartReason};

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

/// A dir for `PATH` with `git`, and a fake `gh` when `gh` is true. Only
/// the server says who is a worker (01M3NT4M159EHN5W8JRTQ417N4).
fn bin(gh: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    std::os::unix::fs::symlink(git.trim(), dir.path().join("git")).unwrap();
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
    isolated::write_program(&path, format!("#!/bin/sh\n{body}\n"));
}

/// A fake `gh` in `dir`: the open milestone Wave 3 with 1 closed issue,
/// its open issues `numbers`, and no pull request.
fn wave_gh(dir: &Path, numbers: &[u64]) {
    let issues: Vec<String> = numbers
        .iter()
        .map(|n| format!(r#"{{"number": {n}, "title": "t", "milestone": {{"title": "Wave 3"}}}}"#))
        .collect();
    let wave = format!(
        r#"[{{"number":3,"title":"Wave 3","open_issues":{},"closed_issues":1}}]"#,
        numbers.len()
    );
    script(
        dir,
        "gh",
        &format!(
            "case \"$1\" in\n  api) echo '{wave}' ;;\n  pr) echo '[]' ;;\n  *) echo '[{}]' ;;\nesac",
            issues.join(", "),
        ),
    );
}

/// A riff-server that needs a sign-in, and the sign-in of its owner,
/// the person brett, in the secret files of the `RIFF_HOME` `home`.
async fn start_server_with_sign_in(home: &Path) -> (riff_server::Service, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = riff_server::Service::new(riff_server::auth::Config {
        require_sign_in: true,
        ..riff_server::auth::Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let secrets = home.join("secrets");
    let key = riff_core::dpop::Key::generate();
    let name = riff_client::device::secret_name(&url);
    riff_client::secrets::file_set(&secrets, &name, &key.to_secret()).unwrap();
    let pair = service
        .admit("brett@comotechnologies.io", false, &key.thumbprint())
        .await
        .unwrap();
    let sign_in = riff_client::login::SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: u64::MAX,
        riff_id: service.riff_id(),
    };
    let name = riff_client::login::secret_name(&url);
    let json = serde_json::to_string(&sign_in).unwrap();
    riff_client::secrets::file_set(&secrets, &name, &json).unwrap();
    (service, url)
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
    let uri = format!("riff://mike@{host}/como-technologies/riff?session={id}");
    live_at(server, uri.parse().unwrap()).await;
}

/// Opens a watch for the session `uri`, so that the session is live
/// until the test ends.
async fn live_at(server: &str, uri: SessionUri) {
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
    for (id, args) in [("a1", &["lead"][..]), ("a1", &["resume", "--riff"][..])] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
    let b2: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    Api::new(server).register_as(&b2, true).await.unwrap();
    for (id, args) in [
        ("b2", &["claim", "issue-12"][..]),
        ("b2", &["status", "tests"][..]),
        ("c3", &["status", "merge"][..]),
        ("c3", &["blocked", "waits for a review"][..]),
    ] {
        output(riff(server, dir, Some(id), path, args)).await;
    }
    for (host, id) in [("pangolin", "a1"), ("thelio", "b2"), ("pangolin", "c3")] {
        live(server, host, id).await;
    }
}

/// A session `id` of mike on pangolin at `place`, `OWNER/REPO` or
/// `OWNER/REPO#WORKTREE`, with the claim `item` in the thread of its
/// repository. It is live until the test ends.
async fn session_at(server: &str, place: &str, id: &str, item: Option<&str>) {
    session_of(server, "mike@pangolin", place, id, item).await;
}

/// A session `id` of `who`, `USER@HOST`, at `place`, as
/// [`session_at`].
async fn session_of(server: &str, who: &str, place: &str, id: &str, item: Option<&str>) {
    let (repo, worktree) = place.split_once('#').unwrap_or((place, ""));
    let fragment = if worktree.is_empty() {
        String::new()
    } else {
        format!("#{worktree}")
    };
    let uri: SessionUri = format!("riff://{who}/{repo}?session={id}{fragment}")
        .parse()
        .unwrap();
    let api = Api::new(server);
    api.register(&uri).await.unwrap();
    if let Some(item) = item {
        let thread = uri.default_thread().unwrap();
        api.claim(&uri, &thread, item).await.unwrap();
    }
    live_at(server, uri).await;
}

/// The worker `b2` of [`three_sessions`] clears its context: a start
/// with a fresh context, as the start hook sends after `/clear`.
async fn clear_b2(server: &str) {
    let b2: SessionUri = "riff://mike@thelio/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    let api = Api::new(server);
    api.start(&b2, StartReason::Clear, true).await.unwrap();
}

/// `line` with each time in seconds, for example `0s` or `12s`, as `Ns`:
/// a slow machine can take a second more.
fn secs(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let word_start = start == 0 || !chars[start - 1].is_alphanumeric();
        let word_end = i + 1 >= chars.len() || !chars[i + 1].is_alphanumeric();
        if i > start && word_start && chars.get(i) == Some(&'s') && word_end {
            out.push_str("Ns");
            i += 1;
        } else if i > start {
            out.extend(&chars[start..i]);
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The lines of the tree, after the header and the board, with each
/// time in seconds as `Ns`.
fn rows(top: &str) -> Vec<String> {
    top.rsplit("\n\n")
        .next()
        .unwrap()
        .lines()
        .map(secs)
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

/// The first word of each row of the tree that is not a detail line:
/// a person, a host, a repository or a session ID, in the order of the
/// tree.
fn heads(top: &str) -> Vec<String> {
    rows(top)
        .iter()
        .filter(|r| r.contains("─ ") || !r.starts_with([' ', '│']))
        .map(|r| {
            let rest = r.rsplit("─ ").next().unwrap();
            rest.split_whitespace().next().unwrap().to_owned()
        })
        .collect()
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
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)\n  claimed: #12\n\n"),
        "{top}"
    );
    let rows = rows(&top);
    assert_eq!(
        rows,
        [
            "mike  online  3 sessions: 1 busy, 1 idle, 1 blocked, 1 claim",
            "├─ pangolin  › riff  2 sessions: 1 lead; 1 idle, 1 blocked",
            "│  ├─ c3  blocked",
            "│  │    waits for a review (Ns ago)",
            "│  └─ a1  lead  idle",
            "│       monitoring work for Ns",
            "└─ thelio  › riff  1 session: 1 worker; 1 busy, 1 claim",
            // thelio runs a worker and tells no numbers
            // (01M515KSZHC9Y2XQSS73PZ27R0).
            "   │  no numbers",
            "   └─ b2  worker  busy",
            "        working on #12 Show the wave",
            "        Ns ago: tests",
        ],
        "a local pane is no worker; blocked comes first: {top}"
    );
}

/// A fake riff daemon of riff 1.8 in the folder `daemon`: it answers each
/// ask with a status that has a session of another daemon of its riff of
/// peers. It gives the count of the asks.
async fn old_daemon(daemon: &Path) -> Arc<AtomicUsize> {
    use tokio::io::AsyncWriteExt;
    let listener = riff::daemon::listen(daemon).await.unwrap();
    let asks = Arc::new(AtomicUsize::new(0));
    let count = asks.clone();
    let status = serde_json::json!({
        "say": "status", "pid": 7, "build": "1.8.0", "since_ms": 0, "dir": daemon,
        "sessions": [], "key": "0a1b", "riff": "0a1b", "owner": "mike",
        "riff_sessions": [{
            "uri": "riff://brett@kadomony/como-technologies/riff?session=77e0aaaa-1&claim=issue-14#issue-14",
            "live": true,
        }],
    });
    tokio::spawn(async move {
        while let Ok((mut conn, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            let _ = conn.write_all(format!("{status}\n").as_bytes()).await;
        }
    });
    asks
}

/// 01M53NGW6F5HP1DZ9QRGP50MGT: `riff top` shows the sessions of
/// riff-server only. It asks no riff daemon, so a session of another
/// daemon of a riff of peers of riff 1.6 to 1.8 shows nowhere.
#[tokio::test(flavor = "multi_thread")]
async fn top_shows_no_session_of_another_daemon() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let asks = old_daemon(&dir.join("daemon")).await;

    let top = output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["top", "--once"],
    ))
    .await;
    assert!(
        rows(&top).iter().any(|row| row.contains("a1  lead")),
        "{top}"
    );
    for other in ["brett", "kadomony", "77e0aaaa", "issue-14"] {
        assert!(!top.contains(other), "{other}: {top}");
    }
    assert_eq!(asks.load(Ordering::SeqCst), 0);
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

/// A fake server with each kind of session: the lead, a worker,
/// a session in a container, the session of `riff workers host` and the
/// session of the riff daemon. Each one has its tag in `riff top` and
/// `riff who`, and the line of a host counts each kind
/// (01M510MG2YSETB2CCAS8XDHPZ0, 01M510MG5J92PYDWZSDQS8HMWK,
/// 01M589HH6HCWYJP1ZC7MRAXZ6E). The idle host and daemon do not say
/// `ready for work` (01M589HH8Z1TCYDCZHQQHTYWMK). The session in a container has no
/// watch: the keep-alive of the daemon makes it live, and the end of
/// its container makes it gone (01M510MG0CKDXAKF6YJ768K7E8).
#[tokio::test(flavor = "multi_thread")]
async fn each_kind_of_session_has_its_tag_and_the_host_line_counts_them() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    for (id, args) in [("a1", &["lead"][..]), ("a1", &["resume", "--riff"][..])] {
        output(riff(&server, dir, Some(id), bin.path(), args)).await;
    }
    live(&server, "pangolin", "a1").await;
    let api = Api::new(&server);
    let uri = |id: &str| -> SessionUri {
        format!("riff://mike@thelio/como-technologies/riff?session={id}")
            .parse()
            .unwrap()
    };
    let w1 = uri("w1");
    api.register_as(&w1, true).await.unwrap();
    live_at(&server, w1).await;
    let c1 = uri("c1#issue-12");
    api.start(&c1, StartReason::Process, true).await.unwrap();
    let thread = c1.default_thread().unwrap();
    assert!(api.claim(&c1, &thread, "issue-12").await.unwrap().granted);
    api.alive_in_container(&c1).await.unwrap();
    // The host never registers: its watch makes it known.
    let h1 = uri("h1");
    live_at(&server, h1.clone()).await;
    let host = riff_client::machines::HostStatus {
        limit: 2,
        floor: 4,
        deaths: 0,
        machine: None,
        disk: None,
        monitor: None,
        more: 0,
        workers: vec![],
    };
    let step = riff_core::wire::Status { step: host.line() };
    api.status(&h1, &step).await.unwrap();
    // The daemon says so in its status (01M589HH6HCWYJP1ZC7MRAXZ6E).
    let d1 = uri("d1");
    live_at(&server, d1.clone()).await;
    let step = riff::daemon::remote::status_line(812, "3.2.0", 0);
    api.status(&d1, &riff_core::wire::Status { step })
        .await
        .unwrap();

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let lead_line = "├─ pangolin  › riff  1 session: 1 lead; 1 idle";
    assert!(rows(&top).iter().any(|r| r == lead_line), "{top}");
    assert_eq!(session(&top, "a1")[0], "│  └─ a1  lead  idle", "{top}");
    for (id, head) in [
        ("c1", "   ├─ c1  #issue-12  container  busy"),
        ("d1", "   ├─ d1  daemon  idle"),
        ("h1", "   ├─ h1  host  idle"),
        ("w1", "   └─ w1  worker  idle"),
    ] {
        assert_eq!(session(&top, id)[0], head, "{top}");
    }
    // The daemon and the host take no claims (01M589HH8Z1TCYDCZHQQHTYWMK).
    assert_eq!(
        session(&top, "d1")[1],
        "   │    hosts containers for Ns",
        "{top}"
    );
    assert_eq!(
        session(&top, "h1")[1],
        "   │    hosts workers for Ns",
        "{top}"
    );
    // A pipe has 80 columns: riff cuts the end of the line with `…`.
    let host_line = "└─ thelio  › riff  4 sessions: 1 container, 1 worker, 1 host, 1 daemon; ";
    assert!(rows(&top).iter().any(|r| r.starts_with(host_line)), "{top}");

    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let line = |id: &str| {
        who.lines()
            .find(|l| l.contains(&format!("({id})")))
            .unwrap_or_else(|| panic!("{id}: {who}"))
            .to_owned()
    };
    for (id, state, tag) in [
        ("c1", "busy", "container"),
        ("d1", "idle", "daemon"),
        ("h1", "idle", "host"),
        ("w1", "idle", "worker"),
    ] {
        let line = line(id);
        let words: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(words[2..4], [state, tag], "{id}: {who}");
    }
    assert!(line("d1").contains("  hosts containers for "), "{who}");
    assert!(line("h1").contains("  hosts workers for "), "{who}");

    // The daemon ends the session at the end of its container: gone.
    api.end(&c1).await.unwrap();
    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(!top.contains("─ c1  "), "{top}");
    let host_line = "└─ thelio  › riff  3 sessions: 1 worker, 1 host, 1 daemon; 3 idle";
    assert!(rows(&top).iter().any(|r| r == host_line), "{top}");
}

/// A fake server with four workers on thelio: one runs an older build,
/// one runs a riff whose file was replaced, one runs the build of the
/// server, and one runs an older riff that names no riff in its
/// keep-alive. Only the first two get the mark `↑`, and the line of the
/// host counts them (01M514C142K5BM2746F1D9Y448,
/// 01M514C16VG9PD70A8424J173B). The older riff gives no error.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_runs_an_old_riff_has_the_mark_and_the_host_line_counts_it() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    for (id, args) in [("a1", &["lead"][..]), ("a1", &["resume", "--riff"][..])] {
        output(riff(&server, dir, Some(id), bin.path(), args)).await;
    }
    live(&server, "pangolin", "a1").await;
    let api = Api::new(&server);
    let binary = |build: &str, replaced| RiffBinary {
        build: build.into(),
        replaced,
    };
    let this = riff_core::build::VERSION;
    let old = "1.3.1 aaaaaaaaaaaa 2026-10-14T10:00:00Z";
    for (id, binary) in [
        ("w1", Some(binary(old, false))),
        ("w2", Some(binary(this, true))),
        ("w3", Some(binary(this, false))),
        ("w4", None),
    ] {
        let uri: SessionUri = format!("riff://mike@thelio/como-technologies/riff?session={id}")
            .parse()
            .unwrap();
        api.register_as(&uri, true).await.unwrap();
        live_at(&server, uri.clone()).await;
        match binary {
            Some(binary) => api.alive_as(&uri, binary).await.unwrap(),
            // An older riff: its keep-alive names no riff.
            None => api.alive(&uri).await.unwrap(),
        };
    }

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    for (id, head) in [
        ("w1", "   ├─ w1  ↑  worker  idle"),
        ("w2", "   ├─ w2  ↑  worker  idle"),
        ("w3", "   ├─ w3  worker  idle"),
        ("w4", "   └─ w4  worker  idle"),
    ] {
        assert_eq!(session(&top, id)[0], head, "{top}");
    }
    let host_line = "└─ thelio  › riff  4 sessions: 4 workers; 2 to update; 4 idle";
    assert!(rows(&top).iter().any(|r| r == host_line), "{top}");
    // The lead named no riff: no mark, and no count on its host.
    assert_eq!(session(&top, "a1")[0], "│  └─ a1  lead  idle", "{top}");
    let lead_line = "├─ pangolin  › riff  1 session: 1 lead; 1 idle";
    assert!(rows(&top).iter().any(|r| r == lead_line), "{top}");
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

/// The author of a pull request released its item at the verify
/// request. The board shows an item with no claim whose pull request
/// waits for a verify or for the merge in `verify`, not in `free`
/// (01M3Z9N6X92KT051P10CKKV7EK). An item with a failed verify is free.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_with_no_claim_and_an_open_pull_request_is_in_verify() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    let issue =
        |n: u64| format!(r#"{{"number": {n}, "title": "t", "milestone": {{"title": "Wave 3"}}}}"#);
    let pull = |n: u64, issue: u64, checks: &str| {
        format!(
            r#"{{"number": {n}, "headRefName": "worktree-issue-{issue}", "headRefOid": "1a2b3c4d", "isDraft": false, "statusCheckRollup": [{checks}]}}"#
        )
    };
    let status = |state: &str| format!(r#"{{"context": "riff/verify", "state": "{state}"}}"#);
    script(
        bin.path(),
        "gh",
        &format!(
            "case \"$1\" in\n  pr) echo '[{}, {}, {}]' ;;\n  *) echo '[{}, {}, {}, {}]' ;;\nesac",
            pull(40, 12, ""),
            pull(41, 13, &status("SUCCESS")),
            pull(42, 14, &status("FAILURE")),
            issue(12),
            issue(13),
            issue(14),
            issue(15),
        ),
    );
    live(&server, "pangolin", "a1").await;
    for args in [&["lead"][..], &["resume", "--riff"]] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)\n  free: #14 #15\n  verify: #12 #13\n\n"),
        "{top}"
    );
}

/// The board of a paused repository ends its wave line with `paused`
/// and has no group `free`: nobody can claim its items
/// (01M588G5K0B58QXD35AA5MTZ6E).
#[tokio::test(flavor = "multi_thread")]
async fn the_board_of_a_paused_repository_has_no_free_items() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    let issue =
        |n: u64| format!(r#"{{"number": {n}, "title": "t", "milestone": {{"title": "Wave 3"}}}}"#);
    script(
        bin.path(),
        "gh",
        &format!(
            "case \"$1\" in\n  pr) echo '[]' ;;\n  *) echo '[{}, {}]' ;;\nesac",
            issue(12),
            issue(13),
        ),
    );
    live(&server, "pangolin", "a1").await;
    for args in [
        &["lead"][..],
        &["resume", "--riff"],
        &["claim", "issue-12"],
        &["pause"],
    ] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)  paused\n  claimed: #12\n\n"),
        "{top}"
    );
}

/// A person with no session runs `riff wave list` in the clone: the
/// repository comes from the folder, as in `riff top`, so the list
/// shows the items of the current wave in the groups of the board
/// (01M58ATZ408RBCX0PM0D0ZCY4B).
#[tokio::test(flavor = "multi_thread")]
async fn a_person_sees_the_groups_of_the_wave_in_riff_wave_list() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    wave_gh(bin.path(), &[12, 13, 14]);
    live(&server, "pangolin", "a1").await;
    for args in [
        &["lead"][..],
        &["resume", "--riff"],
        &["claim", "issue-12"],
        &["plan", "hold", "issue-13", "a reading at 19:40 UTC"],
    ] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }

    let mut list = riff(&server, dir, None, bin.path(), &["wave", "list"]);
    list.env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    let list = output(list).await;
    assert_eq!(
        list,
        "Wave 3  current  3 open, 1 closed\n  free: #14\n  held: #13 (a reading at 19:40 UTC)\n  \
         claimed: #12\n"
    );
}

/// Against a riff-server that needs a sign-in, the calls of `riff wave
/// list` carry the token of the person, so the list shows the groups of
/// the wave (01M58ATZ408RBCX0PM0D0ZCY4B). With no token, the server
/// refuses the calls, and the list has only the milestones.
#[tokio::test(flavor = "multi_thread")]
async fn riff_wave_list_signs_in_to_a_riff_server_that_needs_a_sign_in() {
    let dir = repo();
    let dir = dir.path();
    let (_service, server) = start_server_with_sign_in(dir).await;
    let bin = bin(false);
    wave_gh(bin.path(), &[12]);

    let mut list = riff(&server, dir, None, bin.path(), &["wave", "list"]);
    list.env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    assert_eq!(
        output(list).await,
        "Wave 3  current  1 open, 1 closed\n  free: #12\n"
    );
}

/// The `gh` calls of `riff top` share the token of one read
/// (01M54510SFK1Z9VPFYBYYSV10F): one `gh auth token`, then the token in
/// `GH_TOKEN` of each call.
#[tokio::test(flavor = "multi_thread")]
async fn the_gh_calls_of_top_share_the_token_of_one_read() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    let log = bin.path().join("gh.log");
    script(
        bin.path(),
        "gh",
        &format!(
            "echo \"${{GH_TOKEN:-none}} $1 $2\" >> {}\n\
             case \"$1 $2\" in\n  'auth token') echo gho_one_read ;;\n  'pr list') echo '[]' ;;\n  \
             *) echo '[{{\"number\": 12, \"title\": \"t\", \"milestone\": {{\"title\": \"Wave 3\"}}}}]' ;;\nesac",
            log.display()
        ),
    );
    live(&server, "pangolin", "a1").await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(top.contains("Wave 3 (como-technologies/riff)"), "{top}");
    let calls = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        calls,
        "none auth token\ngho_one_read issue list\ngho_one_read pr list\n"
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
        ["working on #12", "Ns ago: tests"],
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
    // The holds of the boards are a read too (01M58ATZ1F42DWNNB24C2HJPEY).
    let reads = ["/v1/riff", "/v1/who", "/v1/plan/show", "/v1/tail"];
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
    for want in [
        "riff top",
        "riff top --once",
        "riff top --user brett",
        "riff top --host pangolin",
        "riff top --repo como-technologies/strata",
        "riff top --by repo",
    ] {
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

/// The example table of "See what each session does" in How It Works
/// has the form of the output: a board for each repository, the four
/// levels of the tree, the short form, the counts, and the worktree on
/// each session line, and the mark of a session that runs an old riff
/// with its count (01M3WNHCD659FH3Z5VYYH69WWR,
/// 01M42KHN33M4K13GKTX2WM6CMM, 01M42KHN80V49HDDZF953HXDT0,
/// 01M514C142K5BM2746F1D9Y448, 01M514C16VG9PD70A8424J173B).
#[test]
fn the_book_example_has_the_four_level_tree() {
    let page = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let part = page
        .split("\n## See what each session does\n")
        .nth(1)
        .unwrap();
    let example = part.split("```text\n").nth(1).unwrap();
    let example = example.split("```").next().unwrap();
    for board in [
        "\nWave 3 (como-technologies/riff)\n",
        "\nWave 5 (como-technologies/strata)\n",
    ] {
        assert!(example.contains(board), "{board}: {example}");
    }
    let mut worktrees: Vec<&str> = Vec::new();
    for line in example.lines() {
        assert!(line.chars().count() <= 80, "{line}");
        let Some(rest) = line.split("─ ").nth(1) else {
            continue;
        };
        let words: Vec<&str> = rest.split("  ").collect();
        // A session line starts with its ID. Each other row has counts.
        if words[0].len() == 8 {
            worktrees.extend(words.iter().copied().filter(|w| w.starts_with('#')));
        } else {
            assert!(line.contains(" session"), "{line}");
        }
    }
    assert_eq!(
        worktrees,
        ["#issue-88", "#issue-7", "#issue-9", "#issue-8"],
        "{example}"
    );
    for row in [
        "\nbrett  online  › kadomony  › strata  1 session: 1 worker; 1 busy, 1 claim\n",
        "\n├─ pangolin  › riff  2 sessions: 1 container, 1 worker; 2 busy, 2 claims\n",
        "\n│  └─ 9c0d1e2f  #issue-9  container  busy\n",
        "\n└─ thelio  4 sessions: 2 leads; 1 to update; 3 idle, 1 blocked, 1 claim\n",
        "\n   ├─ riff  4 sessions: 2 idle, 1 blocked, 1 claim\n",
        "\n   │  ├─ 4e54d4e5  ↑  lead  idle\n",
        "\n   └─ strata  1 session: 1 idle\n",
    ] {
        assert!(example.contains(row), "{row}: {example}");
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
    // The pause is the one of the repository, and top says who set it
    // (01M3XAHZJAF6YVDJ7WX74X8RBX).
    assert!(
        top.contains("como-technologies/riff by the session mike/a1"),
        "{top}"
    );
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
/// After the release of its last claim it shows `must clear`, until it
/// starts with a fresh context. Then it shows the time since that start
/// (01M3X9XC99KY4RQY36A7CYWY11).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_with_no_claim_shows_must_clear_then_idle_not_its_old_step() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let release = ["release", "issue-12"];
    let released = output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    // The reply to the release carries the ask to clear.
    assert!(
        released.contains("riff clears your context when your turn ends"),
        "{released}"
    );

    let top = ["top", "--once"];
    let before = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let head = &session(&before, "b2")[0];
    assert!(head.ends_with("  worker  must clear"), "{before}");
    let must = ["must clear its context before its next claim"];
    assert_eq!(detail(&before, "b2"), must, "{before}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let b2 = who.lines().find(|l| l.contains("(b2)")).unwrap();
    assert!(b2.contains("  must clear  worker  "), "{who}");
    // A claim before the clear is refused.
    let mut claim = riff(&server, dir, Some("b2"), bin.path(), &["claim", "issue-12"]);
    let refused = claim.output().unwrap();
    assert!(!refused.status.success());
    let why = String::from_utf8_lossy(&refused.stderr).into_owned();
    let text = "clear your context first: end your turn and riff clears it, or type /clear";
    assert!(why.contains(text), "{why}");

    clear_b2(&server).await;
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(session(&top, "b2")[0].ends_with("  worker  idle"), "{top}");
    let ready = ["ready for work for Ns", "fresh start Ns ago"];
    assert_eq!(detail(&top, "b2"), ready, "{top}");
    assert!(
        top.contains("\nWave 3 (como-technologies/riff)\n  free: #12\n"),
        "{top}"
    );
}

/// Each session is under the row of its repository, and its line names
/// its worktree. Each line fits in 80 columns. Each repository has its
/// own board, and a claim in another repository is not on it
/// (01M3WNHCD659FH3Z5VYYH69WWR, 01M42KHN33M4K13GKTX2WM6CMM,
/// 01M42KHN80V49HDDZF953HXDT0).
#[tokio::test(flavor = "multi_thread")]
async fn sessions_of_two_repositories_show_their_repository_and_worktree() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    let release = ["release", "issue-12"];
    output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    clear_b2(&server).await;
    // The first session of mike in strata is its lead.
    let strata = "como-technologies/strata";
    session_at(&server, strata, "d4", Some("issue-12")).await;
    session_at(
        &server,
        &format!("{strata}#issue-88"),
        "e5",
        Some("issue-88"),
    )
    .await;
    session_at(&server, "como-technologies/riff#issue-12", "f6", None).await;

    let top = ["top", "--once", "--color", "never"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert!(!top.contains('\x1b'), "{top:?}");
    for line in top.lines() {
        assert!(line.chars().count() <= 80, "{line:?} in\n{top}");
    }
    let heads: Vec<String> = ["a1", "c3", "d4", "e5", "f6", "b2"]
        .iter()
        .map(|id| session(&top, id)[0].clone())
        .collect();
    assert_eq!(
        heads,
        [
            "│  │  ├─ a1  lead  idle",
            "│  │  ├─ c3  blocked",
            "│     ├─ d4  lead  busy",
            "│     └─ e5  #issue-88  busy",
            "│  │  └─ f6  #issue-12  idle",
            "   └─ b2  worker  idle",
        ],
        "{top}"
    );
    let rows = rows(&top);
    for row in [
        "├─ pangolin  5 sessions: 2 leads; 2 busy, 2 idle, 1 blocked, 2 claims",
        "│  ├─ riff  3 sessions: 2 idle, 1 blocked",
        "│  └─ strata  2 sessions: 2 busy, 2 claims",
        "└─ thelio  › riff  1 session: 1 worker; 1 idle",
    ] {
        assert!(rows.iter().any(|r| r == row), "{row}: {top}");
    }
    assert!(
        top.contains("\n\nWave 3 (como-technologies/riff)\n  free: #12\n\nWave 3 (como-technologies/strata)\n  claimed: #12\n\n"),
        "the claims of strata are not on the board of riff: {top}"
    );

    // `riff who` names the same places.
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    for name in [
        "mike@pangolin:strata (d4)",
        "mike@pangolin:strata#issue-88 (e5)",
        "mike@pangolin:riff#issue-12 (f6)",
    ] {
        assert!(who.contains(name), "{name}: {who}");
    }
}

/// `riff top --user`, `--host`, `--repo` and `--by repo` each show only
/// the matching rows, in the order of the tree, and only the boards of
/// their repositories (01M42KHNCBMBCT3TFBYWE339H5,
/// 01M42KHN33M4K13GKTX2WM6CMM).
#[tokio::test(flavor = "multi_thread")]
async fn the_flags_of_top_show_only_the_matching_rows() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    script(
        bin.path(),
        "gh",
        r#"case "$*" in
  *strata*) echo '[{"number": 88, "title": "Strata item", "milestone": {"title": "Wave 5"}}]' ;;
  *) echo '[{"number": 12, "title": "Show the wave", "milestone": {"title": "Wave 3"}}]' ;;
esac"#,
    );
    three_sessions(&server, dir, bin.path()).await;
    let strata = "como-technologies/strata";
    let g7 = format!("{strata}#issue-88");
    session_of(&server, "brett@kadomony", &g7, "g7", Some("issue-88")).await;
    session_of(&server, "brett@kadomony", strata, "h8", None).await;
    session_at(&server, strata, "d4", None).await;

    let top = |flags: &[&str]| {
        let mut args = vec!["top", "--once"];
        args.extend(flags);
        output(riff(&server, dir, Some("a1"), bin.path(), &args))
    };
    let riff_board = "\nWave 3 (como-technologies/riff)\n";
    let strata_board = "\nWave 5 (como-technologies/strata)\n  claimed: #88\n";

    let all = top(&[]).await;
    assert!(
        all.contains(riff_board) && all.contains(strata_board),
        "{all}"
    );

    let brett = top(&["--user", "brett"]).await;
    assert_eq!(heads(&brett), ["brett", "g7", "h8"], "{brett}");
    assert!(
        rows(&brett)[0].starts_with("brett  online  › kadomony  › strata  2 sessions: "),
        "{brett}"
    );
    assert!(
        brett.contains(strata_board) && !brett.contains(riff_board),
        "{brett}"
    );
    assert!(!brett.contains("blocked  c3"), "{brett}");

    let pangolin = top(&["--host", "pangolin"]).await;
    let want = ["mike", "riff", "c3", "a1", "strata", "d4"];
    assert_eq!(heads(&pangolin), want, "{pangolin}");

    let repo = top(&["--repo", strata]).await;
    assert_eq!(heads(&repo), ["brett", "g7", "h8", "mike", "d4"], "{repo}");
    assert!(
        repo.contains(strata_board) && !repo.contains(riff_board),
        "{repo}"
    );

    let both = top(&["--user", "mike", "--repo", strata]).await;
    assert_eq!(heads(&both), ["mike", "d4"], "{both}");

    let by_repo = top(&["--by", "repo"]).await;
    let want = [
        "riff", "pangolin", "c3", "a1", "thelio", "b2", "strata", "brett", "g7", "h8", "mike", "d4",
    ];
    assert_eq!(heads(&by_repo), want, "{by_repo}");
    assert!(
        rows(&by_repo)[0].starts_with("riff  › mike  online  3 sessions: "),
        "{by_repo}"
    );
}

/// When the repositories of the sessions have more than one owner, each
/// repository line has `OWNER/REPO` (01M3WNHCD659FH3Z5VYYH69WWR).
#[tokio::test(flavor = "multi_thread")]
async fn two_owners_show_the_owner_of_each_repository() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;
    session_at(&server, "acme/strata#issue-88", "d4", Some("issue-88")).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let rows = rows(&top);
    for row in [
        "│  ├─ acme/strata  1 session: 1 busy, 1 claim",
        "│  │  └─ d4  #issue-88  lead  busy",
        "│  └─ como-technologies/riff  2 sessions: 1 idle, 1 blocked",
        "└─ thelio  › como-technologies/riff  1 session: 1 worker; 1 busy, 1 claim",
    ] {
        assert!(rows.iter().any(|r| r == row), "{row}: {top}");
    }
}

/// A session with no claim and no status is `idle` in `riff top` and in
/// `riff who`, with no status call. An idle lead monitors work. An idle
/// worker is ready for work (01M3QB6CJ1XCQG5B1BVR8AF3B4).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_no_status_is_idle() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    three_sessions(&server, dir, bin.path()).await;

    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    assert_eq!(detail(&top, "a1"), ["monitoring work for Ns"], "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let a1 = who.lines().find(|l| l.contains("(a1)")).unwrap();
    assert!(a1.contains("  idle  "), "{who}");
    assert!(secs(a1).ends_with("  monitoring work for Ns"), "{who}");

    let release = ["release", "issue-12"];
    output(riff(&server, dir, Some("b2"), bin.path(), &release)).await;
    clear_b2(&server).await;
    let top = ["top", "--once"];
    let top = output(riff(&server, dir, Some("a1"), bin.path(), &top)).await;
    let ready = ["ready for work for Ns", "fresh start Ns ago"];
    assert_eq!(detail(&top, "b2"), ready, "{top}");
    let who = ["who", "--color", "never"];
    let who = output(riff(&server, dir, Some("a1"), bin.path(), &who)).await;
    let b2 = who.lines().find(|l| l.contains("(b2)")).unwrap();
    let ready = "  ready for work for Ns  fresh start Ns ago";
    assert!(secs(b2).ends_with(ready), "{who}");
}

/// A lead that calls only the command line has a row in `riff top`,
/// with its status (01M48VDGQ5KETKPM4G6TKTC2MB).
#[tokio::test(flavor = "multi_thread")]
async fn a_lead_on_the_command_line_has_a_row() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    for args in [
        &["lead"][..],
        &["resume", "--riff"],
        &["status", "plan the wave"],
    ] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }

    let top = output(riff(&server, dir, None, bin.path(), &["top", "--once"])).await;
    assert!(
        session(&top, "a1")[0].ends_with("─ a1  lead  idle"),
        "{top}"
    );
    assert_eq!(
        detail(&top, "a1"),
        ["monitoring work for Ns", "Ns ago: plan the wave"],
        "{top}"
    );
}

/// `riff top` shows the state of a lead from its facts
/// (01M48VDS8RKJS9HG3KSEYGBFGV): `busy` in a turn, `waiting` for its
/// person, `idle` when nothing goes on. It shows a long step with its
/// age (01M48VDGTD40P8RBZMS0XB5M9N).
#[tokio::test(flavor = "multi_thread")]
async fn top_shows_the_state_of_a_lead_and_its_long_step() {
    use riff_core::wire::Activity;

    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(true);
    for args in [&["lead"][..], &["resume", "--riff"]] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }
    let a1: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a1"
        .parse()
        .unwrap();
    live_at(&server, a1.clone()).await;
    let api = Api::new(&server);
    let top = || async { output(riff(&server, dir, None, bin.path(), &["top", "--once"])).await };
    let state = |top: &str| {
        session(top, "a1")[0]
            .rsplit("  ")
            .next()
            .unwrap()
            .to_owned()
    };

    let tool = Activity {
        tool: Some("Bash: deploy the stage".into()),
        turn: true,
        secs: 0,
    };
    api.alive_with(&a1, Some(tool), None).await.unwrap();
    let step = ["step", "start", "live window"];
    output(riff(&server, dir, Some("a1"), bin.path(), &step)).await;
    let busy = top().await;
    assert_eq!(state(&busy), "busy", "{busy}");
    assert_eq!(
        detail(&busy, "a1"),
        ["runs Bash: deploy the stage for Ns", "live window for Ns"],
        "{busy}"
    );

    let wait = ["blocked", "run riff owner --take"];
    output(riff(&server, dir, Some("a1"), bin.path(), &wait)).await;
    let waiting = top().await;
    assert_eq!(state(&waiting), "waiting", "{waiting}");
    assert_eq!(
        detail(&waiting, "a1"),
        [
            "waiting for mike: run riff owner --take (Ns ago)",
            "live window for Ns"
        ],
        "{waiting}"
    );
    assert!(
        !waiting.contains("blocked"),
        "a lead that waits is not red: {waiting}"
    );

    let ended = Activity {
        tool: None,
        turn: false,
        secs: 0,
    };
    api.alive_with(&a1, Some(ended), Some(0)).await.unwrap();
    output(riff(
        &server,
        dir,
        Some("a1"),
        bin.path(),
        &["step", "done"],
    ))
    .await;
    let idle = top().await;
    assert_eq!(state(&idle), "idle", "{idle}");
    assert_eq!(detail(&idle, "a1"), ["monitoring work for Ns"], "{idle}");
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
    output(riff(&server, dir, Some("s00"), bin.path(), &["lead"])).await;
    output(riff(
        &server,
        dir,
        Some("s00"),
        bin.path(),
        &["resume", "--riff"],
    ))
    .await;
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
        let status = ["blocked", long];
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

/// 01M5924014WN63RCHJWGZS4AFZ, 01M592403Q4TNZEDNR76C7NK9M: `riff wave
/// move` and `riff wave end --move-open-to-backlog` change the milestone
/// of the items with `gh issue edit`, and refuse an item that a session
/// claims.
#[tokio::test(flavor = "multi_thread")]
async fn riff_wave_move_and_end_move_the_items_and_refuse_a_claimed_item() {
    let (server, _) = start_server().await;
    let dir = repo();
    let dir = dir.path();
    let bin = bin(false);
    script(
        bin.path(),
        "gh",
        &r#"echo "gh $*" >> LOG
case "$*" in
  'api repos/{owner}/{repo}/milestones?state=open&per_page=100')
    echo '[{"number":2,"title":"Backlog","open_issues":9,"closed_issues":0},{"number":3,"title":"Wave 3","open_issues":2,"closed_issues":1},{"number":4,"title":"Wave 4: Next","open_issues":0,"closed_issues":0}]' ;;
  'api repos/{owner}/{repo}/issues?milestone=3&state=open&per_page=100') echo '[{"number":13},{"number":14}]' ;;
  'api -X PATCH '*) echo '{"number":3,"title":"Wave 3","open_issues":0,"closed_issues":3}' ;;
  'issue edit '*) ;;
  *) exit 1 ;;
esac"#
        .replace("LOG", &bin.path().join("gh.log").display().to_string()),
    );
    live(&server, "pangolin", "a1").await;
    for args in [&["lead"][..], &["resume", "--riff"], &["claim", "issue-12"]] {
        output(riff(&server, dir, Some("a1"), bin.path(), args)).await;
    }
    let run = |args: &[&str]| {
        let mut cmd = riff(&server, dir, None, bin.path(), args);
        cmd.env_remove("RIFF_SESSION")
            .env_remove("CLAUDE_CODE_SESSION_ID");
        async move {
            let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
                .await
                .unwrap();
            (
                out.status.success(),
                String::from_utf8_lossy(&out.stdout).into_owned()
                    + &String::from_utf8_lossy(&out.stderr),
            )
        }
    };
    let log = || std::fs::read_to_string(bin.path().join("gh.log")).unwrap_or_default();

    let (ok, said) = run(&["wave", "move", "13", "14", "--to", "4"]).await;
    assert!(ok, "{said}");
    assert_eq!(said, "Moved #13 #14 to Wave 4: Next.\n");
    let (ok, said) = run(&["wave", "move", "13", "--to", "Backlog"]).await;
    assert!(ok, "{said}");
    assert_eq!(said, "Moved #13 to Backlog.\n");
    let edits: Vec<String> = log()
        .lines()
        .filter(|l| l.starts_with("gh issue edit "))
        .map(Into::into)
        .collect();
    assert_eq!(
        edits,
        [
            "gh issue edit 13 --milestone Wave 4: Next",
            "gh issue edit 14 --milestone Wave 4: Next",
            "gh issue edit 13 --milestone Backlog",
        ]
    );

    let before = log().lines().count();
    let (ok, said) = run(&["wave", "move", "12", "13", "--to", "4"]).await;
    assert!(!ok && said.contains("#12 has a claim."), "{said}");
    let (ok, said) = run(&["wave", "move", "13", "--to", "9"]).await;
    assert!(!ok && said.contains("No open wave matches 9."), "{said}");
    assert!(
        log()
            .lines()
            .skip(before)
            .all(|l| !l.starts_with("gh issue edit "))
    );

    let (ok, said) = run(&["wave", "end", "3", "--move-open-to-backlog"]).await;
    assert!(ok, "{said}");
    assert_eq!(said, "Moved #13 #14 to Backlog. Ended the wave Wave 3.\n");
    assert!(log().contains("gh api -X PATCH repos/{owner}/{repo}/milestones/3 -f state=closed"));
}
