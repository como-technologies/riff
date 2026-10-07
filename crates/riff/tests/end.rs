//! A session leaves the riff when it ends (R205): `riff mcp` sends the
//! end call when its stdin closes or a signal stops it, and the end
//! hook sends it for each reason but `clear` (R168). A keep-alive is
//! not a call (R204).

use isolated::Isolated;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use riff::api::Api;
use riff::mcp::Tools;
use riff_core::name::{SessionUri, ThreadName};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A riff command for mike on pangolin, with its own local directory
/// and no keyring.
fn riff(api: &Api, run: &Path, args: &[&str]) -> Command {
    let mut cmd = Isolated::shared().tokio_riff();
    cmd.args(args)
        .current_dir(run)
        .env("RIFF_HOME", run)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd
}

/// Starts `riff mcp` for the session `id`.
fn mcp(api: &Api, run: &Path, id: &str) -> Child {
    riff(api, run, &["mcp"])
        .env("RIFF_SESSION", id)
        .spawn()
        .unwrap()
}

/// Runs `riff hook session-end` with `input` on stdin.
async fn end_hook(api: &Api, run: &Path, input: &str) {
    let mut hook = riff(api, run, &["hook", "session-end"]).spawn().unwrap();
    let mut stdin = hook.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await.unwrap();
    drop(stdin);
    assert!(hook.wait().await.unwrap().success());
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

/// `riff watch` sends a keep-alive while it runs: an open watch stream
/// is no sign of life (01M3WG240PNMQYZ7TX6Z7ZF6M9). A keep-alive brings
/// a gone session back (R207), so the test sees it in `riff who`.
#[tokio::test]
async fn the_keep_alive_of_a_watch_is_a_sign_of_life() {
    let api = start_server().await;
    let me = uri("riff://mike@pangolin/como-technologies/riff?session=e5");
    api.register(&me).await.unwrap();
    api.end(&me).await.unwrap();
    assert!(!shown(&api).await.contains(&"e5".to_owned()));

    let alive = riff::api::keep_alive(&api, &me, Duration::from_millis(100));
    let _ = tokio::time::timeout(Duration::from_millis(1_000), alive).await;
    assert!(shown(&api).await.contains(&"e5".to_owned()));
}

fn brett() -> SessionUri {
    uri("riff://brett@heron/como-technologies/riff?session=b2")
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The session IDs that `who` shows now.
async fn shown(api: &Api) -> Vec<String> {
    api.who(&brett(), false)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|s| s.uri.who().session().map(str::to_owned))
        .collect()
}

/// Waits until `who` shows `id` or not, for at most 10 seconds.
async fn wait_until_shown(api: &Api, id: &str, want: bool) {
    let span = isolated::Span::start();
    while shown(api).await.iter().any(|s| s == id) != want {
        assert!(
            span.within(Duration::from_secs(10)),
            "{id}: shown is not {want}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The URI of `id` in `who`, with its place.
async fn session(api: &Api, id: &str) -> SessionUri {
    api.who(&brett(), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.uri.who().session() == Some(id))
        .unwrap()
        .uri
}

/// Resumes the new riff as the person mike.
async fn resume(api: &Api) {
    let mike = uri("riff://mike@pangolin/como-technologies/riff");
    api.set_riff(&mike, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_session_leaves_when_the_stdin_of_riff_mcp_closes() {
    let api = start_server().await;
    let run = tempfile::tempdir().unwrap();
    let mut child = mcp(&api, run.path(), "e1");
    wait_until_shown(&api, "e1", true).await;
    let me = session(&api, "e1").await;
    let thread = me.default_thread().unwrap_or_else(repo);
    api.register(&brett()).await.unwrap();
    resume(&api).await;
    assert!(api.claim(&me, &thread, "issue-12").await.unwrap().granted);

    drop(child.stdin.take());
    wait_until_shown(&api, "e1", false).await;
    let taken = api.claim(&brett(), &thread, "issue-12").await.unwrap();
    assert!(taken.granted, "the claim is free at once");
    // No MCP client started the session, so the exit status is an error.
    child.wait().await.unwrap();
}

#[tokio::test]
async fn a_session_leaves_when_a_signal_stops_riff_mcp() {
    let api = start_server().await;
    let run = tempfile::tempdir().unwrap();
    let mut child = mcp(&api, run.path(), "e2");
    wait_until_shown(&api, "e2", true).await;
    let pid = child.id().unwrap().to_string();
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .unwrap();
    assert!(killed.success());
    wait_until_shown(&api, "e2", false).await;
    child.wait().await.unwrap();
}

#[tokio::test]
async fn the_end_hook_ends_the_session_but_not_after_clear() {
    let api = start_server().await;
    let run = tempfile::tempdir().unwrap();
    let me = uri("riff://mike@pangolin/como-technologies/riff?session=e3");
    api.register(&me).await.unwrap();
    resume(&api).await;
    api.claim(&me, &repo(), "issue-7").await.unwrap();

    end_hook(&api, run.path(), r#"{"session_id":"e3","reason":"clear"}"#).await;
    assert!(shown(&api).await.contains(&"e3".to_owned()));
    assert_eq!(session(&api, "e3").await.claims(), ["issue-7"]);

    let exit = r#"{"session_id":"e3","reason":"prompt_input_exit"}"#;
    end_hook(&api, run.path(), exit).await;
    assert!(!shown(&api).await.contains(&"e3".to_owned()));
    assert!(session(&api, "e3").await.claims().is_empty());

    // A call brings the session back, with the same ID and no claims.
    api.register(&me).await.unwrap();
    assert!(shown(&api).await.contains(&"e3".to_owned()));
    assert!(session(&api, "e3").await.claims().is_empty());
}

#[tokio::test]
async fn a_keep_alive_is_not_a_call() {
    let api = start_server().await;
    let me = uri("riff://mike@pangolin/como-technologies/riff?session=e4");
    api.register(&me).await.unwrap();
    let tools = Tools::new(api.clone(), me);
    let alive = tools.keep_alive_every(Duration::from_millis(100));
    tokio::time::sleep(Duration::from_millis(2_200)).await;
    alive.abort();
    let listed = api.who(&brett(), false).await.unwrap();
    let e4 = listed
        .iter()
        .find(|s| s.uri.who().session() == Some("e4"))
        .unwrap();
    assert!(e4.idle_secs >= 2, "idle {}", e4.idle_secs);
}
