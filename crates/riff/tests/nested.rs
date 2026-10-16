//! An agent inside a riff session is no session of the riff
//! (01M517N75C0Z5BAFQAWRN3YTYB), over real HTTP: for example a
//! `claude -p` that a Bash command of a worker starts (#822). The test
//! process is the agent of the session: it holds the record of its
//! `riff mcp`. A shell below it is the inner agent. It runs riff with
//! `CLAUDE_PID` set to its own PID, as Claude Code gives it to each
//! hook, and with the `RIFF_SESSION` of the worker, which it inherits.
//! The hooks and `riff mcp` of the inner agent leave the session and
//! its claim as they are. With no record above it, the same start hook
//! frees the claim: that is how the claim of #814 went.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use isolated::Isolated;
use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::RiffState;
use rmcp::ServiceExt;
use tokio::io::AsyncWriteExt;

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// The session of the outer agent.
const ID: &str = "w1";

fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

/// The files of `riff mcp` for the riff home `run`.
fn files(run: &Path) -> PathBuf {
    run.join("state")
}

/// `riff ARGS` in `run`, as the child of a shell: the shell is the
/// inner agent.
fn inner(api: &Api, run: &Path, args: &[&str]) -> tokio::process::Command {
    let env = Isolated::shared();
    let mut cmd = tokio::process::Command::from(env.command("sh"));
    cmd.arg("-c")
        .arg(r#"CLAUDE_PID=$$ "$@"; exit $?"#)
        .arg("sh")
        .arg(env.riff_path())
        .args(args)
        .current_dir(run)
        .env("RIFF_HOME", run)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", api.base())
        .env("RIFF_SESSION", ID)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    cmd
}

/// Runs the hook `event` of the inner agent with `input` on stdin.
/// Returns its output.
async fn hook(api: &Api, run: &Path, event: &str, input: &str) -> String {
    let mut child = inner(api, run, &["hook", event]).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await.unwrap();
    drop(stdin);
    let out = child.wait_with_output().await.unwrap();
    assert!(out.status.success(), "{event}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The context of the output of a start hook.
fn context(output: &str) -> String {
    let output: serde_json::Value = serde_json::from_str(output).unwrap();
    output["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The claims of the session in `who`. `None` when it is not live.
async fn claims(api: &Api) -> Option<Vec<String>> {
    api.who(&uri("x0"), false)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.uri.who().session() == Some(ID))
        .map(|s| s.uri.claims().to_vec())
}

const START: &str = r#"{"session_id":"n1","source":"startup"}"#;
const END: &str = r#"{"session_id":"n1","reason":"other"}"#;
const STOP: &str = r#"{"session_id":"n1"}"#;

#[tokio::test]
async fn the_hooks_of_an_agent_inside_a_session_keep_its_claims() {
    let api = start_server().await;
    let run = tempfile::tempdir().unwrap();
    api.register(&uri(ID)).await.unwrap();
    api.set_riff(&uri(ID), RiffState::Running).await.unwrap();
    assert!(api.claim(&uri(ID), &repo(), "issue-814").await.unwrap().granted);
    let me = std::process::id();
    let record = riff::local::record(&files(run.path()), me, ID)
        .unwrap()
        .expect("free");

    let start = hook(&api, run.path(), "session-start", START).await;
    assert_eq!(context(&start), riff::hook::nested_line(ID));
    for (event, input) in [("stop", STOP), ("session-end", END)] {
        let out = hook(&api, run.path(), event, input).await;
        assert_eq!(out, "", "{event}");
    }
    assert_eq!(claims(&api).await, Some(vec!["issue-814".to_owned()]));
    let other = api.claim(&uri("w2"), &repo(), "issue-814").await.unwrap();
    assert!(!other.granted, "two sessions hold one item");

    // With no record above it, the agent is the session: its start
    // frees the claim.
    drop(record);
    let span = isolated::Span::start();
    while riff::local::recorded(&files(run.path()), me).is_some() {
        assert!(span.within(Duration::from_secs(20)), "the record stays");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let start = hook(&api, run.path(), "session-start", START).await;
    let context = context(&start);
    assert!(
        context.contains(&format!("freed your claims: issue-814 in {}", repo())),
        "{context}"
    );
    assert_eq!(claims(&api).await, Some(Vec::new()));
}

#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_of_an_agent_inside_a_session_has_no_tool_and_makes_no_call() {
    let api = start_server().await;
    let run = tempfile::tempdir().unwrap();
    let _record = riff::local::record(&files(run.path()), std::process::id(), ID)
        .unwrap()
        .expect("free");

    let mut child = inner(&api, run.path(), &["mcp"]).spawn().unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let info = client.peer_info().unwrap();
    let line = riff::hook::nested_line(ID);
    assert_eq!(info.instructions.as_deref(), Some(line.as_str()));
    assert!(info.capabilities.tools.is_none(), "{info:?}");
    let tools = client.list_all_tools().await.unwrap_or_default();
    assert!(tools.is_empty(), "{tools:?}");
    client.cancel().await.unwrap();
    let ended = isolated::in_time(Duration::from_secs(20), child.wait()).await;
    assert!(ended.is_ok(), "riff mcp did not end");
    // It registered no session.
    let all = api.who(&uri("x0"), true).await.unwrap();
    assert!(
        all.iter().all(|s| s.uri.who().session() != Some(ID)),
        "{all:?}"
    );
}
