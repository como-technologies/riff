//! `riff outside` on the side of `riff` (#614): an admin decides in a
//! terminal, never in a session (01M4DA9PJ0MJPBQRTA79CVXEA2), and a
//! process with no sandbox asks nothing (01M4DA9PFR6V3K3FE1568277H3).
//! The tests of the server side are in `riff-server/tests/outside.rs`,
//! and the run of the broker in `riff::broker`.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::routing::post;
use isolated::Isolated;
use serde_json::{Value, json};

use crate::forge::{clone, serve};

/// A fake server that keeps each body of `path`, and replies `reply`.
async fn server(path: &'static str, reply: Value) -> (String, Arc<Mutex<Vec<Value>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let url = serve(axum::Router::new().route(
        path,
        post(move |Json(body): Json<Value>| async move {
            s.lock().unwrap().push(body);
            Json(reply)
        }),
    ))
    .await;
    (url, seen)
}

fn request(state: &str) -> Value {
    json!({
        "id": "7f3a9c21",
        "by": "riff://mike@pangolin/como-technologies/riff?session=a6cf",
        "command": ["sudo", "true"],
        "cwd": "/w",
        "reason": "the test needs root",
        "state": state,
        "decided_by": "mike",
    })
}

async fn run(cmd: std::process::Command) -> (bool, String, String) {
    let mut cmd = cmd;
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[tokio::test]
async fn no_session_approves_denies_or_lists_and_riff_calls_no_server() {
    let (url, seen) = server("/v1/outside/decide", request("approved")).await;
    let env = Isolated::new();
    let runs: [(&[&str], (&str, &str)); 4] = [
        (&["approve", "7f3a9c21"], ("RIFF_SESSION", "a6cf")),
        (&["approve", "7f3a9c21"], ("RIFF_WORKER", "1")),
        (&["deny", "7f3a9c21"], ("CLAUDE_CODE_SESSION_ID", "a6cf")),
        (&["list"], ("RIFF_SESSION", "a6cf")),
    ];
    for (args, (name, value)) in runs {
        let mut cmd = env.riff();
        cmd.arg("outside")
            .args(args)
            .env("RIFF_SERVER", &url)
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env(name, value);
        let (ok, _, stderr) = run(cmd).await;
        assert!(!ok, "{args:?} {name}: {stderr}");
        assert!(
            stderr.contains("not in a worker or an agent session"),
            "{args:?} {name}: {stderr}"
        );
    }
    assert!(seen.lock().unwrap().is_empty(), "riff called no server");
}

#[tokio::test]
async fn riff_outside_ask_with_no_sandbox_asks_nothing() {
    let (url, seen) = server("/v1/outside/ask", request("asked")).await;
    let env = Isolated::new();
    let mut cmd = env.riff();
    cmd.args(["outside", "ask", "--reason", "a test", "--", "true"])
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a6cf")
        .env_remove("RIFF_BROKER");
    let (ok, _, stderr) = run(cmd).await;
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("This process has no sandbox"), "{stderr}");
    assert!(seen.lock().unwrap().is_empty(), "riff called no server");
}

#[tokio::test]
async fn an_admin_approves_in_a_terminal() {
    let (url, seen) = server("/v1/outside/decide", request("approved")).await;
    let env = Isolated::new();
    let repo = clone();
    let mut cmd = env.riff();
    cmd.args(["outside", "approve", "7f3a9c21"])
        .current_dir(repo.path())
        .env("RIFF_SERVER", &url)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin");
    let (ok, stdout, stderr) = run(cmd).await;
    assert!(ok, "{stdout} {stderr}");
    assert!(
        stdout.contains("the request 7f3a9c21 of mike/a6cf (`sudo true`) is approved"),
        "{stdout}"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["id"], "7f3a9c21");
    assert_eq!(seen[0]["approve"], true);
    assert!(
        !seen[0]["me"].as_str().unwrap().contains("session="),
        "a person decides, not a session: {}",
        seen[0]["me"]
    );
}
