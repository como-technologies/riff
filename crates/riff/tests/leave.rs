//! A session leaves the riff and joins it again, through a real MCP
//! client, against a real server (01M3MEEFC9ZQVW2KC9FNJ75MTY,
//! 01M3MEEFETT9A0DRWBKQTG77Z2, 01M3MEEFKX14QCQM0F9ZYW93PP).

use std::path::Path;
use std::process::Command;

use riff::api::Api;
use riff::mcp::Tools;
use riff_core::name::SessionUri;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::json;

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// Connects an MCP client to `tools`.
async fn connect(api: &Api, tools: Tools, me: &SessionUri) -> RunningService<RoleClient, ()> {
    api.register(me).await.unwrap();
    api.set_riff(me, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    ().serve(client_io).await.unwrap()
}

async fn call(
    client: &RunningService<RoleClient, ()>,
    tool: &str,
    args: serde_json::Value,
) -> (String, bool) {
    let params = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(params).await.unwrap();
    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    (text, result.is_error == Some(true))
}

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a1#issue-12";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b2";

/// Runs git in `dir` with no git settings of the user or the machine,
/// for example a signature for each commit.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A clone of a bare remote, with one commit on `main`, on the branch
/// `branch`, and one file that is not committed.
fn clone_on(root: &Path, branch: &str) -> std::path::PathBuf {
    let remote = root.join("remote.git");
    let work = root.join("work");
    git(
        root,
        &["init", "--quiet", "--bare", "-b", "main", "remote.git"],
    );
    git(
        root,
        &["clone", "--quiet", remote.to_str().unwrap(), "work"],
    );
    git(&work, &["config", "user.name", "Mike"]);
    git(&work, &["config", "user.email", "mike@example.com"]);
    // The leave tool runs git with the settings of the user. The settings
    // of the clone win over a signature for each commit.
    git(&work, &["config", "commit.gpgsign", "false"]);
    git(
        &work,
        &["commit", "--quiet", "--allow-empty", "-m", "start"],
    );
    git(&work, &["push", "--quiet", "-u", "origin", "main"]);
    git(&work, &["remote", "set-head", "origin", "main"]);
    if branch != "main" {
        git(&work, &["checkout", "--quiet", "-b", branch]);
    }
    std::fs::write(work.join("notes.txt"), "half done").unwrap();
    work
}

fn lists(who: &str, session: &str) -> bool {
    who.contains(&format!("?session={session}"))
}

#[tokio::test]
async fn a_leave_pushes_the_work_frees_the_claims_and_a_join_comes_back() {
    let root = tempfile::tempdir().unwrap();
    let run = tempfile::tempdir().unwrap();
    let work = clone_on(root.path(), "worktree-issue-12");
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let brett: SessionUri = BRETT.parse().unwrap();
    let tools = Tools::new(api.clone(), mike.clone())
        .in_local(Some(run.path().to_owned()))
        .in_dir(work.clone());
    let m = connect(&api, tools, &mike).await;
    let b = connect(&api, Tools::new(api.clone(), brett.clone()), &brett).await;

    let (claimed, _) = call(&m, "claim", json!({ "item": "issue-12" })).await;
    assert!(claimed.contains("You hold issue-12"), "{claimed}");

    let (left, error) = call(&m, "leave", json!({})).await;
    assert!(!error, "{left}");
    assert!(
        left.contains("pushed the branch worktree-issue-12"),
        "{left}"
    );
    assert!(left.contains("freed your claims: issue-12"), "{left}");
    assert!(riff::local::left(run.path(), "a1"));

    // The WIP commit is on the remote, with the file.
    let remote = root.path().join("remote.git");
    let log = git(&remote, &["log", "-1", "--format=%s", "worktree-issue-12"]);
    assert_eq!(log, riff::leave::WIP_MESSAGE);
    let files = git(
        &remote,
        &["show", "--name-only", "--format=", "worktree-issue-12"],
    );
    assert_eq!(files, "notes.txt");
    assert!(git(&work, &["status", "--porcelain"]).is_empty());

    // Mike is gone, and his claim is free.
    let (who, _) = call(&b, "who", json!({})).await;
    assert!(!lists(&who, "a1"), "{who}");
    let (told, error) = call(&b, "tell", json!({ "session": "a1", "body": "hi" })).await;
    assert!(error, "{told}");
    let (claimed, _) = call(&b, "claim", json!({ "item": "issue-12" })).await;
    assert!(claimed.contains("You hold issue-12"), "{claimed}");

    // Each tool but join refuses, and names /riff:join.
    for tool in m.list_all_tools().await.unwrap() {
        if tool.name == "join" {
            continue;
        }
        let args = match &*tool.name {
            "post" | "tell" => json!({ "session": "b2", "body": "hi" }),
            "status" => json!({ "step": "x" }),
            "claim" | "release" => json!({ "item": "issue-13" }),
            "move" => json!({ "path": work.to_str().unwrap() }),
            _ => json!({}),
        };
        let (text, error) = call(&m, &tool.name, args).await;
        assert!(error, "{}: {text}", tool.name);
        assert!(text.contains("/riff:join"), "{}: {text}", tool.name);
    }
    // The refusals made no call: Mike is still gone.
    let (who, _) = call(&b, "who", json!({})).await;
    assert!(!lists(&who, "a1"), "{who}");

    let (joined, error) = call(&m, "join", json!({})).await;
    assert!(!error, "{joined}");
    assert!(joined.contains("session=a1"), "{joined}");
    assert!(joined.contains("riff watch --once"), "{joined}");
    assert!(!riff::local::left(run.path(), "a1"));
    let (who, _) = call(&b, "who", json!({})).await;
    assert!(lists(&who, "a1"), "{who}");
    let (told, error) = call(&b, "tell", json!({ "session": "a1", "body": "hi" })).await;
    assert!(!error, "{told}");
    assert!(told.contains("Woke mike@pangolin"), "{told}");
    let (me, error) = call(&m, "whoami", json!({})).await;
    assert!(!error, "{me}");
}

#[tokio::test]
async fn a_leave_on_the_default_branch_keeps_the_claims() {
    let root = tempfile::tempdir().unwrap();
    let work = clone_on(root.path(), "main");
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let m = connect(
        &api,
        Tools::new(api.clone(), mike.clone()).in_dir(work.clone()),
        &mike,
    )
    .await;
    call(&m, "claim", json!({ "item": "issue-12" })).await;
    let (text, error) = call(&m, "leave", json!({})).await;
    assert!(error, "{text}");
    assert!(text.contains("still in the riff"), "{text}");
    assert!(text.contains("default branch main"), "{text}");
    let (who, _) = call(&m, "who", json!({})).await;
    assert!(who.contains("claim=issue-12"), "{who}");
}

#[tokio::test]
async fn a_leave_with_no_claim_pushes_nothing() {
    let not_git = tempfile::tempdir().unwrap();
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let m = connect(
        &api,
        Tools::new(api.clone(), mike.clone()).in_dir(not_git.path().to_owned()),
        &mike,
    )
    .await;
    let (left, error) = call(&m, "leave", json!({})).await;
    assert!(!error, "{left}");
    assert!(!left.contains("pushed"), "{left}");
}

/// A resume starts a new `riff mcp`. It finds the record, and its tools
/// refuse (01M3MEEFH79XXNZW6DWSPTEW2A).
#[tokio::test]
async fn a_leave_holds_for_new_tools() {
    let run = tempfile::tempdir().unwrap();
    riff::local::leave(run.path(), "a1").unwrap();
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let tools = Tools::new(api.clone(), mike.clone()).in_local(Some(run.path().to_owned()));
    assert!(tools.left());
    let other: SessionUri = BRETT.parse().unwrap();
    let fresh = Tools::new(api, other).in_local(Some(run.path().to_owned()));
    assert!(!fresh.left(), "a new session joins as usual");
}
