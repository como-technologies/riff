//! How a worker ends, how it waits with no work, and what its lead sees
//! (01M493YZVZGA7TSRJH6F67VN0H, 01M3JQC8ETHRAWSJPHMKA062SQ,
//! 01M3K0AXMCVRST7HYH4DM8B3AN, 01M3K0AXRNA0F2920E9QCSDFQZ). A fake
//! `claude` runs in place of Claude Code.

use isolated::{Isolated, Span, in_time};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use riff::api::Api;
use riff_core::name::SessionUri;

const LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=lead1";

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A git repository with a GitHub origin, so the repository is
/// `como-technologies/riff`.
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
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success();
        assert!(ok);
    }
    dir
}

/// A fake `claude` in `dir` that runs `script`, and counts its starts in
/// the file `starts`.
fn fake_claude(dir: &Path, script: &str) -> PathBuf {
    let path = dir.join("claude");
    let body = format!(
        "#!/bin/sh\necho start >> '{}'\n{script}\n",
        dir.join("starts").display()
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// `riff` in `dir` as mike on pangolin, in the worker pane `%5` of the
/// worker session `session`.
fn riff(api: &Api, dir: &Path, session: &str) -> Command {
    let mut cmd = Isolated::shared().riff();
    cmd.current_dir(dir)
        .env("RIFF_SERVER", api.base())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", session)
        .env("RIFF_BIN", Isolated::shared().riff_path())
        .env("TMUX_PANE", "%5")
        .env("RIFF_HOME", dir)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("RIFF_WORKER");
    cmd
}

/// Registers the lead: the first session of mike in the repository.
async fn lead(api: &Api) -> SessionUri {
    let lead: SessionUri = LEAD.parse().unwrap();
    api.register(&lead).await.unwrap();
    lead
}

/// The unread text of the lead.
async fn lead_reads(api: &Api, lead: &SessionUri) -> String {
    riff::text::inbox(&api.inbox(lead, None, false).await.unwrap(), lead)
}

fn starts(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("starts"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// `claude` exits with status 1. The lead gets a note with the pane,
/// the session ID and the exit code (01M493YZVZGA7TSRJH6F67VN0H). The
/// wrapper does not start `claude` again. The exit is a death of the
/// machine (01M493YZZEW1FTDBNA090WT2AG).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_that_exits_tells_the_lead() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let claude = fake_claude(dir.path(), "exit 1");
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .arg("Join the riff.")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(starts(dir.path()), 1);

    let read = lead_reads(&api, &lead).await;
    assert!(
        read.contains("note: worker stopped: pane %5, session w1, exit code 1."),
        "{read}"
    );
    assert!(!read.contains("workers died"), "{read}");
    let now = riff::monitor::now_secs();
    assert_eq!(riff::deaths::count_in(&dir.path().join("state"), now), 1);
    // The crashed worker does not come back in `riff who`.
    let who = api.who(&lead, false).await.unwrap();
    assert!(
        who.iter().all(|s| s.uri.who().session() != Some("w1")),
        "{who:?}"
    );
}

/// 3 workers of the machine died in the last hour. The fourth exit with
/// a fault starts a loop of deaths: the lead gets one message that wakes
/// it (01M493Z02KS82B3CVZEVFA3D6E). An exit with the code 0 is no death.
#[tokio::test(flavor = "multi_thread")]
async fn a_fourth_death_in_an_hour_tells_the_lead_one_time() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let state = dir.path().join("state");
    let now = riff::monitor::now_secs();
    for id in ["d1", "d2", "d3"] {
        riff::deaths::record(&state, id, now - 60).unwrap();
    }
    let ok = fake_claude(dir.path(), "exit 0");
    let out = riff(&api, dir.path(), "w0")
        .args(["workers", "run"])
        .arg(&ok)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(riff::deaths::count_in(&state, now), 3);

    let claude = fake_claude(dir.path(), "exit 1");
    for id in ["w1", "w2"] {
        let out = riff(&api, dir.path(), id)
            .args(["workers", "run"])
            .arg(&claude)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "{out:?}");
    }
    let read = lead_reads(&api, &lead).await;
    let alarm = riff::text::death_loop("pangolin", 4);
    assert_eq!(read.matches(&alarm).count(), 1, "{read}");
    assert!(read.contains("direct with mike@pangolin"), "{read}");
    assert!(!read.contains(&format!("note: {alarm}")), "{read}");
}

/// mike has sessions and a person command on host `b`. A worker on host
/// `a` stops: its message comes from `mike@a`
/// (01M3MWW8KYJ3ZV91X22RBSAF33).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_stopped_message_names_the_host_of_the_worker() {
    let api = start_server().await;
    let lead: SessionUri = "riff://mike@b/como-technologies/riff?session=lead1"
        .parse()
        .unwrap();
    api.register(&lead).await.unwrap();
    let person_on_b: SessionUri = "riff://mike@b".parse().unwrap();
    api.register(&person_on_b).await.unwrap();
    let dir = repo();
    let claude = fake_claude(dir.path(), "exit 1");
    let out = riff(&api, dir.path(), "w1")
        .env("RIFF_HOST", "a")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");

    let inbox = api.inbox(&lead, None, false).await.unwrap();
    let stopped: Vec<_> = inbox
        .iter()
        .flat_map(|t| &t.messages)
        .filter(|m| m.message.body.starts_with("worker stopped"))
        .collect();
    assert_eq!(stopped.len(), 1, "{stopped:?}");
    assert_eq!(
        stopped[0].message.from.short(),
        "mike@a:riff",
        "{stopped:?}"
    );
}

/// The worker gets `RIFF_WORKER=1`.
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_marks_claude_as_a_worker() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!("echo \"$RIFF_WORKER\" > '{}'", seen.display()),
    );
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(std::fs::read_to_string(seen).unwrap().trim(), "1");
}

/// The wrapper gives `claude` each argument as it is, also the flag
/// settings that turn off Remote Control (01M3JV0ZNGKDFMRR9ACT0480V9),
/// the recap (01M3MN0D429T4Q80DYBE9S9XR7) and the plugins with a
/// language server (01M3ZJ1FAF7EJXP9CSET8ZY1K3).
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_gives_claude_the_flag_settings() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!("printf '%s\\n' \"$@\" > '{}'", seen.display()),
    );
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .args([
            "--settings",
            &riff::terminal::worker_settings(&["rust-lsp@m".into()]),
        ])
        .arg(riff::terminal::JOIN)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let seen = std::fs::read_to_string(seen).unwrap();
    let lines: Vec<&str> = seen.lines().collect();
    assert_eq!(
        (lines[0], lines[2]),
        ("--settings", "Join the riff."),
        "{seen}"
    );
    let settings: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    let tmp = dir.path().join("tmp/w1").display().to_string();
    assert_eq!(settings["remoteControlAtStartup"], false);
    assert_eq!(settings["awaySummaryEnabled"], false);
    assert_eq!(settings["enabledPlugins"]["rust-lsp@m"], false);
    assert_eq!(settings["env"]["TMPDIR"], tmp.as_str());
    assert_eq!(settings["env"]["CLAUDE_CODE_TMPDIR"], tmp.as_str());
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert_eq!(deny[..2], ["Bash(riff cloud)", "Bash(riff cloud *)"]);
}

/// A fake home with a dot file and a folder of notes.
fn home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".bashrc"), "").unwrap();
    std::fs::create_dir_all(home.path().join("notes")).unwrap();
    home
}

/// The allow and the deny rules of the flag settings `settings`.
fn rules(settings: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let list = |name: &str| -> Vec<String> {
        let rules = settings["permissions"][name].as_array().unwrap();
        rules
            .iter()
            .map(|r| r.as_str().unwrap().to_owned())
            .collect()
    };
    (list("allow"), list("deny"))
}

/// A rule of `tool` for the absolute path `path`, as Claude Code writes
/// it: `//` and the path.
fn rule(tool: &str, path: &Path, tail: &str) -> String {
    format!("{tool}(/{}{tail})", path.display())
}

/// The wrapper gives `claude` the permission rules of the profile of a
/// worker (01M4BT33R71HXAVQGHFD4ZFGR5, 01M4BT33TPSXJVB6JZDZ3F1GGX,
/// 01M4BT33X0WVVJH7Y6AXSWZEYC, 01M4BT341H1M1N1MT947HXNXDR).
#[tokio::test(flavor = "multi_thread")]
async fn the_wrapper_gives_claude_the_rules_of_the_profile() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let home = home();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!("printf '%s\\n' \"$@\" > '{}'", seen.display()),
    );
    let out = riff(&api, dir.path(), "w1")
        .env("HOME", home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .args(["workers", "run"])
        .arg(&claude)
        .arg(riff::terminal::JOIN)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let seen = std::fs::read_to_string(seen).unwrap();
    let lines: Vec<&str> = seen.lines().collect();
    assert_eq!(
        (lines[0], lines[1]),
        ("Join the riff.", "--settings"),
        "{seen}"
    );
    let settings: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    let (allow, deny) = rules(&settings);
    let worktrees = dir.path().canonicalize().unwrap().join(".claude/worktrees");
    assert!(
        allow.contains(&rule("Edit", &worktrees, "/**")),
        "{allow:?}"
    );
    assert!(
        allow.contains(&rule("Read", &worktrees, "/**")),
        "{allow:?}"
    );
    let h = home.path();
    assert!(
        deny.contains(&rule("Read", &h.join(".bashrc"), "")),
        "{deny:?}"
    );
    assert!(
        deny.contains(&rule("Edit", &h.join("notes"), "/**")),
        "{deny:?}"
    );
    let settings_file = h.join(".claude/settings.json");
    assert!(deny.contains(&rule("Edit", &settings_file, "")), "{deny:?}");
    assert!(deny.contains(&"Edit(//**/.claude/settings.local.json)".to_owned()));
}

/// `riff workers rules` prints the rules that a worker in this clone
/// gets (01M4BT3JQCY5G7YZ373MV5C5JM).
#[tokio::test(flavor = "multi_thread")]
async fn riff_workers_rules_prints_the_rules_of_a_worker() {
    let api = start_server().await;
    let dir = repo();
    let home = home();
    let out = riff(&api, dir.path(), "w1")
        .env("HOME", home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .args(["workers", "rules"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let settings: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let (allow, deny) = rules(&settings);
    let git = dir.path().canonicalize().unwrap().join(".git");
    assert!(allow.contains(&rule("Edit", &git, "/**")), "{allow:?}");
    let bashrc = home.path().join(".bashrc");
    assert!(deny.contains(&rule("Read", &bashrc, "")), "{deny:?}");
}

/// The worker gets a temp folder of its own on disk, in `TMPDIR` and
/// `CLAUDE_CODE_TMPDIR` (01M41VAGJC69S9R2TD1B1EQ4W4). The wrapper
/// deletes it when `claude` ends (01M41VAGQ2VA2Q0VSFJNG4H08W).
#[tokio::test(flavor = "multi_thread")]
async fn the_worker_gets_a_temp_folder_and_its_end_deletes_it() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let seen = dir.path().join("seen");
    let claude = fake_claude(
        dir.path(),
        &format!(
            "echo \"$TMPDIR $CLAUDE_CODE_TMPDIR\" > '{}'\n\
             mkdir -p \"$CLAUDE_CODE_TMPDIR/claude-1000/p/s1/scratchpad\"\n\
             echo build > \"$CLAUDE_CODE_TMPDIR/claude-1000/p/s1/scratchpad/riff\"",
            seen.display()
        ),
    );
    let out = riff(&api, dir.path(), "w1")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let folder = dir.path().join("tmp/w1");
    assert_eq!(
        std::fs::read_to_string(seen).unwrap().trim(),
        format!("{0} {0}", folder.display())
    );
    assert!(!folder.exists(), "the end deletes the temp folder");
    assert!(dir.path().join("tmp").exists());
}

/// `riff workers stop` closes the pane: the stopped worker has no temp
/// folder any more (01M41VAGQ2VA2Q0VSFJNG4H08W).
#[tokio::test(flavor = "multi_thread")]
async fn a_stop_deletes_the_temp_folder() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let claude = fake_claude(dir.path(), "echo x > \"$TMPDIR/rustc.tmp\"\nexec sleep 30");
    let mut child = riff(&api, dir.path(), "w4")
        .args(["workers", "run"])
        .arg(&claude)
        .spawn()
        .unwrap();
    let file = dir.path().join("tmp/w4/rustc.tmp");
    let begin = Instant::now();
    while !file.exists() {
        assert!(begin.elapsed() < Duration::from_secs(60), "no temp file");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let hup = Command::new("kill")
        .args(["-HUP", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(hup.success());
    assert_eq!(child.wait().unwrap().code(), Some(0));
    assert!(!dir.path().join("tmp/w4").exists());
}

/// A child of `claude` that lives after its end keeps the temp folder
/// (01M41VAGMQPBDPV8XPGEKYRXZZ). When it ends, the tidy deletes the
/// folder (01M41VAGVR2PPVAYDN0SWK2F02).
#[tokio::test(flavor = "multi_thread")]
async fn a_live_process_keeps_the_temp_folder_until_the_tidy() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let pid = dir.path().join("pid");
    let claude = fake_claude(
        dir.path(),
        &format!(
            "sleep 60 > /dev/null 2>&1 < /dev/null &\necho $! > '{}'",
            pid.display()
        ),
    );
    let out = riff(&api, dir.path(), "w5")
        .args(["workers", "run"])
        .arg(&claude)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let folder = dir.path().join("tmp/w5");
    let root = dir.path().join("tmp");
    let later = std::time::SystemTime::now() + Duration::from_secs(120);
    assert!(folder.exists(), "the live sleep keeps it");
    assert!(riff::temp::sweep(&root, &riff::temp::users(), later).is_empty());

    let pid = std::fs::read_to_string(pid).unwrap();
    let killed = Command::new("kill").arg(pid.trim()).status().unwrap();
    assert!(killed.success());
    let begin = Instant::now();
    while riff::temp::users()
        .iter()
        .any(|u| u.pid.to_string() == pid.trim())
    {
        assert!(begin.elapsed() < Duration::from_secs(60), "the sleep lives");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        riff::temp::sweep(&root, &riff::temp::users(), later),
        std::slice::from_ref(&folder)
    );
    assert!(!folder.exists());
}

/// A worker with no work keeps its watch, and sets no status. It stays
/// in `riff who`, where riff shows it idle (01M3Q555KC1RKNEC4ZA9HQYJG2),
/// and a request of the lead wakes it.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_of_the_lead_wakes_an_idle_worker() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    // `riff mcp` of a worker registers it as a worker.
    let w2: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w2"
        .parse()
        .unwrap();
    api.register_as(&w2, true).await.unwrap();
    let watch = riff(&api, dir.path(), "w2")
        .args(["watch", "--once"])
        .env("RIFF_WORKER", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // The idle worker is live in `riff who`, and idle. The limit
    // only stops a hang: a slow machine still passes.
    let begin = Instant::now();
    let idle = loop {
        let who = api.who(&lead, false).await.unwrap();
        if let Some(w2) = who
            .into_iter()
            .find(|s| s.uri.who().session() == Some("w2") && s.live)
        {
            break w2;
        }
        assert!(begin.elapsed() < Duration::from_secs(60), "no live w2");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(idle.status.is_none(), "{idle:?}");
    // The riff of this test is paused, and paused wins over idle
    // (01M3QB6CJ1XCQG5B1BVR8AF3B4).
    assert!(idle.uri.claims().is_empty(), "{idle:?}");
    assert_eq!(
        idle.state,
        Some(riff_core::wire::SessionState::Paused),
        "{idle:?}"
    );

    api.tell(&lead, "w2", "request: claim issue-12")
        .await
        .unwrap();
    let out = tokio::task::spawn_blocking(move || watch.wait_with_output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(
        line.contains("riff: mike@pangolin:riff (lead1) wrote to you in a direct message"),
        "{line}"
    );
}

/// `riff workers stop` closes the pane: tmux sends SIGHUP. The wrapper
/// stops `claude` and sends no message.
#[tokio::test(flavor = "multi_thread")]
async fn a_hangup_stops_the_worker_with_no_message() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let dir = repo();
    let claude = fake_claude(dir.path(), "exec sleep 30");
    let mut child = riff(&api, dir.path(), "w3")
        .args(["workers", "run"])
        .arg(&claude)
        .spawn()
        .unwrap();
    while starts(dir.path()) == 0 {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let hup = Command::new("kill")
        .args(["-HUP", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(hup.success());
    let span = Span::start();
    let status = child.wait().unwrap();
    assert!(span.within(Duration::from_secs(10)));
    assert_eq!(status.code(), Some(0));
    assert_eq!(lead_reads(&api, &lead).await, "No unread messages.");
}

/// The server stops an idle worker (01M3Q5A0NKY1FCS0YH6N6YD3GN,
/// 01M3Q5A0QZTSTXHHNYCE8HFJSB): the `riff mcp` of the worker gets the
/// ask in the reply to its keep-alive, and stops its wrapper. The
/// wrapper exits with 0, the session leaves `riff who`, and the lead
/// gets a note (01M3Q5A0WRQT4SGPSD0CQFF011).
#[tokio::test(flavor = "multi_thread")]
async fn the_server_stops_an_idle_worker_through_its_wrapper() {
    let api = start_server().await;
    let lead = lead(&api).await;
    let idle = api.idle(&person(&lead), Some(0), Some(1)).await.unwrap();
    assert_eq!((idle.per_host, idle.after_secs), (0, 1));
    let dir = repo();
    let claude = fake_claude(dir.path(), "exec \"$RIFF_BIN\" mcp");
    let mut child = riff(&api, dir.path(), "w6")
        .args(["workers", "run"])
        .arg(&claude)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // The worker registers as a worker. Then it has a watch, as each
    // worker does.
    let w6: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w6"
        .parse()
        .unwrap();
    let span = Span::start();
    while !api
        .who(&lead, false)
        .await
        .unwrap()
        .iter()
        .any(|s| s.uri.who() == w6.who() && s.worker)
    {
        assert!(span.within(Duration::from_secs(30)), "no worker w6");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _watch = api.watch(&w6).await.unwrap();

    let status = tokio::task::spawn_blocking(move || {
        let begin = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return (status, child.wait_with_output().unwrap());
            }
            assert!(begin.elapsed() < Duration::from_secs(60), "w6 runs");
            std::thread::sleep(Duration::from_millis(100));
        }
    })
    .await
    .unwrap();
    let err = String::from_utf8_lossy(&status.1.stderr).into_owned();
    assert_eq!(status.0.code(), Some(0), "{err}");
    assert!(err.contains(riff::text::IDLE_STOP), "{err}");

    let span = Span::start();
    while api
        .who(&lead, false)
        .await
        .unwrap()
        .iter()
        .any(|s| s.uri.who() == w6.who())
    {
        assert!(span.within(Duration::from_secs(10)), "w6 in who");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let read = lead_reads(&api, &lead).await;
    assert!(
        read.contains("workers: the server stops the idle worker w6 on pangolin."),
        "{read}"
    );
    assert!(!read.contains("worker stopped"), "{read}");
}

/// The lead wakes an idle worker after the server asks it to stop, but
/// before its next keep-alive. The end of the watch takes the ask back,
/// so the worker goes on (01M3Q5A0NKY1FCS0YH6N6YD3GN).
#[tokio::test(flavor = "multi_thread")]
async fn a_wake_of_the_lead_takes_back_the_ask_to_stop() {
    use futures::StreamExt;

    let api = start_server().await;
    let lead = lead(&api).await;
    api.idle(&person(&lead), Some(0), Some(1)).await.unwrap();
    let w7: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w7"
        .parse()
        .unwrap();
    api.register_as(&w7, true).await.unwrap();
    let mut watch = Box::pin(api.watch(&w7).await.unwrap());

    let stopping = async || {
        api.who(&lead, false)
            .await
            .unwrap()
            .iter()
            .any(|s| s.uri.who() == w7.who() && s.stopping)
    };
    let span = Span::start();
    while !stopping().await {
        assert!(span.within(Duration::from_secs(30)), "no ask to stop");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    api.tell(&lead, "w7", "request: claim issue-12")
        .await
        .unwrap();
    let wake = in_time(Duration::from_secs(10), watch.next()).await;
    assert!(matches!(wake, Ok(Some(Ok(_)))), "no wake");
    drop(watch);

    let span = Span::start();
    while stopping().await {
        assert!(span.within(Duration::from_secs(10)), "the ask stays");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!api.alive(&w7).await.unwrap().stop);
}

/// A worker whose `riff mcp` ended, for example at a self-update, has
/// only its watch. The watch gets the ask to stop in the reply to its
/// keep-alive, and stops the wrapper (01M4385Z5BN03E6HTEB5GQVZ8X). So the
/// worker is gone within one minute.
#[tokio::test(flavor = "multi_thread")]
async fn the_watch_of_a_worker_with_no_riff_mcp_stops_its_wrapper() {
    let api = start_server().await;
    let lead = lead(&api).await;
    api.idle(&person(&lead), Some(0), Some(1)).await.unwrap();
    let w8: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w8"
        .parse()
        .unwrap();
    api.register_as(&w8, true).await.unwrap();
    let dir = repo();
    let claude = fake_claude(dir.path(), "exec \"$RIFF_BIN\" watch --once");
    let child = riff(&api, dir.path(), "w8")
        .args(["workers", "run"])
        .arg(&claude)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let begin = Instant::now();
    let (status, output) = tokio::task::spawn_blocking(move || {
        let mut child = child;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return (status, child.wait_with_output().unwrap());
            }
            assert!(begin.elapsed() < Duration::from_secs(60), "w8 runs");
            std::thread::sleep(Duration::from_millis(100));
        }
    })
    .await
    .unwrap();
    let out = String::from_utf8_lossy(&output.stdout).into_owned();
    let err = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(status.code(), Some(0), "{out}{err}");
    assert!(out.contains(riff::text::IDLE_STOP), "{out}{err}");
    assert!(!err.contains("worker stopped"), "{err}");
    let read = lead_reads(&api, &lead).await;
    assert_eq!(
        read.matches("the server stops the idle worker w8").count(),
        1,
        "{read}"
    );
}

/// The server asks an idle worker to stop while its repository is
/// paused. The pause and the resume wake the worker, and each wake
/// takes the ask back. The server asks again each time, but the lead
/// gets one note (01M4385Z039RCFSKWFPWZAETTX). After the resume, the
/// reply to a keep-alive still asks the worker to stop.
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_and_a_resume_give_one_note_and_the_stop_holds() {
    use futures::StreamExt;
    use riff::api::PauseScope;
    use riff_core::wire::RiffState;

    let api = start_server().await;
    let lead = lead(&api).await;
    api.set_riff(&lead, RiffState::Running).await.unwrap();
    api.idle(&person(&lead), Some(0), Some(1)).await.unwrap();
    let w9: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w9"
        .parse()
        .unwrap();
    api.register_as(&w9, true).await.unwrap();

    let stopping = async || {
        api.who(&lead, false)
            .await
            .unwrap()
            .iter()
            .any(|s| s.uri.who() == w9.who() && s.stopping)
    };
    let wait_for_the_ask = async || {
        let span = Span::start();
        while !stopping().await {
            assert!(span.within(Duration::from_secs(30)), "no ask to stop");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };

    let mut watch = Box::pin(api.watch(&w9).await.unwrap());
    wait_for_the_ask().await;
    for state in [RiffState::Paused, RiffState::Running] {
        api.set_pause(&lead, &PauseScope::Here, state)
            .await
            .unwrap();
        let wake = in_time(Duration::from_secs(10), watch.next()).await;
        assert!(matches!(wake, Ok(Some(Ok(_)))), "no wake at {state:?}");
        drop(watch);
        // The worker reads, and starts its watch again.
        api.inbox(&w9, None, false).await.unwrap();
        watch = Box::pin(api.watch(&w9).await.unwrap());
        wait_for_the_ask().await;
    }

    assert!(api.alive(&w9).await.unwrap().stop, "the stop holds");
    let read = lead_reads(&api, &lead).await;
    assert_eq!(
        read.matches("the server stops the idle worker w9").count(),
        1,
        "{read}"
    );
    drop(watch);
}

/// `riff workers idle` shows the settings of idle workers, and sets
/// them (01M3Q5A0TF9K49V8Z1ZY9NDF74).
#[tokio::test(flavor = "multi_thread")]
async fn riff_workers_idle_shows_and_sets_the_settings() {
    let api = start_server().await;
    lead(&api).await;
    let dir = repo();
    let idle = |args: &[&str]| {
        let out = riff(&api, dir.path(), "lead1")
            .args(["workers", "idle"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        idle(&[]),
        "The server keeps at most 1 idle worker on each host. It stops each other worker \
         that is idle for 60 seconds.\n"
    );
    assert_eq!(
        idle(&["--per-host", "2", "--after", "300"]),
        "The server keeps at most 2 idle workers on each host. It stops each other worker \
         that is idle for 300 seconds.\n"
    );
    let zero = riff(&api, dir.path(), "lead1")
        .args(["workers", "idle", "--after", "0"])
        .output()
        .unwrap();
    assert!(!zero.status.success(), "{zero:?}");
}

/// The start hook tells a session with `RIFF_WORKER=1` that it is a
/// worker, to wait idle with no work, and that its work on an item
/// ends at the verify request (01M3Z9N6AK6W9KCA1MN72X78B6).
#[tokio::test(flavor = "multi_thread")]
async fn the_start_hook_tells_a_worker() {
    let api = start_server().await;
    let dir = repo();
    let context = |worker: bool| {
        let mut cmd = riff(&api, dir.path(), "w5");
        cmd.args(["hook", "session-start"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if worker {
            cmd.env("RIFF_WORKER", "1");
        }
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"session_id":"w5","source":"startup"}"#)
            .unwrap();
        String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
    };
    let worker = context(true);
    assert!(
        worker.contains("You are a worker (RIFF_WORKER=1)"),
        "{worker}"
    );
    assert!(!worker.contains("riff workers done"), "{worker}");
    assert!(
        worker.contains("and you hold no claim, keep your watch running, and end your turn"),
        "{worker}"
    );
    assert!(
        worker.contains(
            "Your work on an item ends at the verify request: write the state on the issue, \
             release the item, and end your turn. Do not wait for the verify."
        ),
        "{worker}"
    );
    assert!(!context(false).contains("You are a worker"));
}

/// The person of `me` on the command line: no session. Only a person
/// changes the settings of idle workers.
fn person(me: &SessionUri) -> SessionUri {
    format!("riff://{}@pangolin", me.who().user())
        .parse()
        .unwrap()
}
