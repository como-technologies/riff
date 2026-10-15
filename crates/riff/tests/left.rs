//! A session that left the riff sends no request from any entry of the
//! client (01M3MEEFETT9A0DRWBKQTG77Z2, 01M3XQVJXWBC3DKAVWBPXPSGZS): the
//! status line, each hook, the watch, each command, the tools and the
//! tasks of `riff mcp`. So no call brings it back in `riff who`. The
//! leave holds over a restart of `riff mcp` and of `riff-server`, and
//! only a join ends it (01M3XQVK05FAT3PR43W8RNEYHY).

use isolated::Isolated;
use std::path::{Path, PathBuf};
use std::process::{Command as Git, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use riff::api::{Api, Left};
use riff::mcp::Tools;
use riff::rollout::{Env, Live};
use riff_core::name::SessionUri;
use riff_core::wire::RiffState;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::store::Memory;

const ID: &str = "a6cf2205-d54a-4c1e-9b1f-2e3d4c5b6a7f";

type Calls = Arc<Mutex<Vec<String>>>;

/// A real riff-server on `store` that records the path of each request
/// in `calls`. It gives the service and its URL.
async fn counting_server(store: Memory, calls: Calls) -> (Service, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease: riff_server::lease::Timing {
            wait: Duration::from_millis(50),
            read_every: Duration::from_millis(50),
            valid_for: Duration::from_millis(500),
            exit_after: Duration::from_secs(1),
            ..Default::default()
        },
        ..Config::new(&url)
    };
    let service = Service::load(config, Arc::new(store)).await.unwrap();
    let router = service.router().layer(axum::middleware::map_request(
        move |r: axum::extract::Request| {
            calls.lock().unwrap().push(r.uri().path().to_owned());
            std::future::ready(r)
        },
    ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// A git repository with a GitHub origin.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/como-technologies/riff.git",
        ][..],
    ] {
        assert!(
            Git::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
    }
    dir
}

/// The directory of the mark of a leave, for the home `dir`.
fn marks(dir: &Path) -> PathBuf {
    dir.join("state")
}

fn me() -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={ID}")
        .parse()
        .unwrap()
}

/// The client of the session [`ID`], with its mark in the home `dir`.
fn client(server: &str, dir: &Path) -> Api {
    Api::new(server).for_session(&marks(dir), ID)
}

/// `riff ARGS` in `dir` with `stdin`, as a process of the agent tool
/// with the session `session` in its environment, or with none.
async fn riff(
    server: &str,
    dir: &Path,
    session: Option<&str>,
    stdin: &str,
    args: &[&str],
) -> Output {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.args(args)
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("RIFF_WORKER")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir)
        .write_stdin(stdin);
    if let Some(id) = session {
        cmd.env("RIFF_SESSION", id);
    }
    tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Runs `riff mcp` of the session [`ID`] for `time`, then closes its
/// stdin, so that it ends.
async fn mcp_for(server: &str, dir: &Path, time: Duration) {
    let mut mcp = Isolated::shared().tokio_riff();
    mcp.arg("mcp")
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", ID)
        .env_remove("RIFF_WORKER")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut mcp = mcp.spawn().unwrap();
    tokio::time::sleep(time).await;
    drop(mcp.stdin.take());
    let ended = isolated::in_time(Duration::from_secs(20), mcp.wait()).await;
    assert!(ended.is_ok(), "riff mcp did not end");
}

/// The sessions that `riff who` of a person shows.
async fn who(server: &str, dir: &Path) -> String {
    let out = riff(server, dir, None, "", &["who", "--long"]).await;
    assert!(out.status.success(), "{out:?}");
    stdout(&out)
}

/// Runs each entry of the client as the session [`ID`], which left the
/// riff. Each one stops with no request, and says that the session
/// left where it has an output.
async fn each_entry(server: &str, dir: &Path) {
    let id = format!(r#""session_id":"{ID}""#);

    let line = riff(server, dir, None, &format!("{{{id}}}"), &["statusline"]).await;
    assert_eq!(stdout(&line), "riff a6cf2205 (left)\n");

    for source in ["startup", "resume", "clear", "compact"] {
        let input = format!(r#"{{{id},"source":"{source}"}}"#);
        let hook = riff(server, dir, None, &input, &["hook", "session-start"]).await;
        assert!(hook.status.success(), "{source}: {hook:?}");
        assert_eq!(stdout(&hook), "", "{source}");
    }
    let input = format!(r#"{{{id},"reason":"logout"}}"#);
    let hook = riff(server, dir, None, &input, &["hook", "session-end"]).await;
    assert!(hook.status.success(), "{hook:?}");
    let hook = riff(server, dir, None, &format!("{{{id}}}"), &["hook", "stop"]).await;
    assert!(hook.status.success(), "{hook:?}");
    let hook = riff(server, dir, None, "", &["hook", "compact", "--session", ID]).await;
    assert!(hook.status.success(), "{hook:?}");

    let watch = riff(server, dir, Some(ID), "", &["watch", "--once"]).await;
    assert!(watch.status.success(), "{watch:?}");
    assert_eq!(stdout(&watch), format!("{}\n", riff::text::WATCH_LEFT));

    for args in [
        &["who"][..],
        &["whoami"][..],
        &["status", "the tests run"][..],
        &["claim", "issue-12"][..],
        &["release", "issue-12"][..],
        &["read"][..],
        &["post", "--to", "user=mike", "hi"][..],
        &["tell", "lead", "hi"][..],
        &["lead"][..],
        &["top", "--once"][..],
    ] {
        let out = riff(server, dir, Some(ID), "", args).await;
        assert!(!out.status.success(), "{args:?}: {out:?}");
        let err = stderr(&out);
        assert!(err.contains(riff::text::LEFT_COMMAND), "{args:?}: {err}");
    }
    // These commands ask the server before the check of the leave in
    // `main`. Only the client stops them.
    for args in [&["workers"][..], &["workers", "start", "1"][..]] {
        let out = riff(server, dir, Some(ID), "", args).await;
        assert!(!stdout(&out).contains(ID), "{args:?}: {out:?}");
    }

    // `riff mcp` with a rollout that looks each second: no register, no
    // look of the rollout, no end call.
    mcp_for(server, dir, Duration::from_millis(2500)).await;
}

#[tokio::test]
async fn a_session_that_left_sends_no_request_from_any_entry_until_it_joins() {
    let calls = Calls::default();
    let store = Memory::default();
    let (service, server) = counting_server(store.clone(), calls.clone()).await;
    let dir = repo();
    let interval = riff(&server, dir.path(), None, "", &["workers", "interval", "1"]).await;
    assert!(interval.status.success(), "{interval:?}");
    let resume = riff(&server, dir.path(), None, "", &["resume", "--riff"]).await;
    assert!(resume.status.success(), "{resume:?}");

    // The session is the lead, so its `riff mcp` runs the rollout.
    let lead = riff(&server, dir.path(), Some(ID), "", &["lead"]).await;
    assert!(lead.status.success(), "{lead:?}");
    assert!(who(&server, dir.path()).await.contains(ID));

    let api = client(&server, dir.path());
    api.leave_riff(&me()).await.unwrap();
    assert!(riff::local::left(&marks(dir.path()), ID));
    assert!(!who(&server, dir.path()).await.contains(ID));

    calls.lock().unwrap().clear();
    each_entry(&server, dir.path()).await;
    let made = calls.lock().unwrap().clone();
    assert!(made.is_empty(), "a left session sent requests: {made:?}");
    assert!(!who(&server, dir.path()).await.contains(ID));

    // riff-server starts again on the same store. The leave is a mark on
    // the machine of the session, so it holds.
    service.shutdown().await.unwrap();
    let again = Calls::default();
    let (_service, server) = counting_server(store, again.clone()).await;
    each_entry(&server, dir.path()).await;
    let made = again.lock().unwrap().clone();
    assert!(made.is_empty(), "a left session sent requests: {made:?}");
    assert!(!who(&server, dir.path()).await.contains(ID));

    // A join brings the session back, with the same session ID.
    let api = client(&server, dir.path());
    api.join_riff(&me(), false).await.unwrap();
    assert!(!riff::local::left(&marks(dir.path()), ID));
    assert!(who(&server, dir.path()).await.contains(ID));
    let input = format!(r#"{{"session_id":"{ID}"}}"#);
    let line = riff(&server, dir.path(), None, &input, &["statusline"]).await;
    assert!(stdout(&line).starts_with("riff a6cf2205"), "{line:?}");
    assert!(!stdout(&line).contains("(left)"), "{line:?}");
    let status = riff(&server, dir.path(), Some(ID), "", &["status", "back"]).await;
    assert!(status.status.success(), "{status:?}");
}

/// The keep-alive, the rollout and the end of the tools of a session
/// that left send no request, also when the caller does not ask
/// `Tools::left` first: the client stops each request.
#[tokio::test]
async fn the_tasks_of_riff_mcp_send_no_request_after_a_leave() {
    let calls = Calls::default();
    let (_service, server) = counting_server(Memory::default(), calls.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let api = client(&server, dir.path());
    let person: SessionUri = "riff://mike@pangolin".parse().unwrap();
    Api::new(&server)
        .set_riff(&person, RiffState::Running)
        .await
        .unwrap();
    api.register(&me()).await.unwrap();
    let tools = Tools::new(api.clone(), me());
    let rollout = Live {
        api: api.clone(),
        me,
        tmux: None,
        claude: "claude".into(),
        gh: Arc::new(riff::pr::Gh::default()),
        off_told: false.into(),
    };
    // The session is the lead: the rollout sees the settings.
    assert!(rollout.seen().await.unwrap().is_some());

    api.leave_riff(&me()).await.unwrap();
    calls.lock().unwrap().clear();
    assert!(tools.left());

    let alive = tools.keep_alive_every(Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(200)).await;
    alive.abort();
    assert!(rollout.seen().await.unwrap().is_none());
    let look = rollout.look().await.unwrap_err();
    assert!(look.is::<Left>(), "{look:#}");
    tools.end().await;
    for call in [
        api.alive(&me()).await.map(drop),
        api.register(&me()).await,
        api.end(&me()).await,
        api.who(&me(), true).await.map(drop),
        api.me(&me()).await.map(drop),
        api.watch(&me()).await.map(drop),
    ] {
        let error = call.unwrap_err();
        assert!(error.is::<Left>(), "{error:#}");
    }
    let made = calls.lock().unwrap().clone();
    assert!(made.is_empty(), "a left session sent requests: {made:?}");

    // Another session of the same machine is in the riff as before.
    let other = Api::new(&server).for_session(&marks(dir.path()), "b2");
    let brett: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=b2"
        .parse()
        .unwrap();
    other.register(&brett).await.unwrap();
    let shown = other.who(&brett, false).await.unwrap();
    assert!(shown.iter().all(|s| s.uri.who() != me().who()), "{shown:?}");

    api.join_riff(&me(), false).await.unwrap();
    assert!(!tools.left());
    let shown = other.who(&brett, false).await.unwrap();
    assert!(shown.iter().any(|s| s.uri.who() == me().who()), "{shown:?}");
}

/// A leave that the server does not get is no leave: the mark goes, and
/// the session stays in the riff (01M3XQVK05FAT3PR43W8RNEYHY).
#[tokio::test]
async fn a_leave_with_no_server_keeps_no_mark() {
    let dir = tempfile::tempdir().unwrap();
    let api = client("http://127.0.0.1:1", dir.path());
    let error = api.leave_riff(&me()).await.unwrap_err();
    assert!(!error.is::<Left>(), "{error:#}");
    assert!(!api.left());

    // A client with no mark cannot keep a leave.
    let error = Api::new("http://127.0.0.1:1")
        .leave_riff(&me())
        .await
        .unwrap_err();
    assert!(error.to_string().contains(riff::text::LEAVE_NO_MARK));
}
