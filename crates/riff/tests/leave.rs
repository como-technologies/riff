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

/// Registers `me` in a running riff, and connects an MCP client to
/// `tools`.
async fn connect(api: &Api, tools: Tools, me: &SessionUri) -> RunningService<RoleClient, ()> {
    api.register(me).await.unwrap();
    api.set_riff(me, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    connect_only(tools).await
}

/// Connects an MCP client to `tools`, with no call to the server.
async fn connect_only(tools: Tools) -> RunningService<RoleClient, ()> {
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
        .in_local(run.path())
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
            "blocked" => json!({ "reason": "x" }),
            "claim" | "release" | "free" => json!({ "item": "issue-13" }),
            "hold" => json!({ "item": "issue-13", "reason": "x" }),
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

/// A leave ends each claim of the session, and puts its tokens on the
/// issue (01M3Y1YP1ZA5TBRA01MKWM3VC6).
#[tokio::test]
async fn a_leave_reports_the_tokens_of_each_claim() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let run = tempfile::tempdir().unwrap();
    let marks = tempfile::tempdir().unwrap();
    // A fake gh: it has no comment, and keeps each body that it gets in
    // the file after `--input`.
    let gh = root.path().join("gh");
    std::fs::write(
        &gh,
        r#"#!/bin/sh
prev=
for a; do
    [ "$prev" = --input ] && cat "$a" >> "$(dirname "$0")/sent"
    prev=$a
done
"#,
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let meter = riff::usage::Meter {
        dir: marks.path().to_owned(),
        gh: riff::pr::Gh::at(&gh),
    };
    let work = clone_on(root.path(), "worktree-issue-12");
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let tools = Tools::new(api.clone(), mike.clone())
        .in_local(run.path())
        .in_dir(work)
        .with_meter(Some(meter));
    let m = connect(&api, tools, &mike).await;

    let transcript = marks.path().join("a1.jsonl");
    riff::usage::saw(marks.path(), "a1", &transcript).unwrap();
    call(&m, "claim", json!({ "item": "issue-12" })).await;
    let at = chrono::Utc::now().to_rfc3339();
    let reply = json!({
        "timestamp": at,
        "message": { "id": "m1", "model": "opus", "usage": { "output_tokens": 7 } },
    });
    std::fs::write(&transcript, reply.to_string()).unwrap();

    let (left, error) = call(&m, "leave", json!({})).await;
    assert!(!error, "{left}");
    assert!(
        left.ends_with(
            "The claim of issue-12 took 7 tokens (input 0, output 7, cache write 0, cache read \
             0). riff put them on #12 as a comment."
        ),
        "{left}"
    );
    let sent = std::fs::read_to_string(root.path().join("sent")).unwrap();
    assert!(
        sent.contains("riff usage: issue-12, work, session a1, "),
        "{sent}"
    );
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
    let run = tempfile::tempdir().unwrap();
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let m = connect(
        &api,
        Tools::new(api.clone(), mike.clone())
            .in_local(run.path())
            .in_dir(not_git.path().to_owned()),
        &mike,
    )
    .await;
    let (left, error) = call(&m, "leave", json!({})).await;
    assert!(!error, "{left}");
    assert!(!left.contains("pushed"), "{left}");
}

/// A resume starts a new `riff mcp`. It finds the mark, and its tools
/// refuse (01M3MEEFH79XXNZW6DWSPTEW2A). `riff who` does not list the
/// session.
#[tokio::test]
async fn a_leave_holds_for_new_tools() {
    let run = tempfile::tempdir().unwrap();
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let brett: SessionUri = BRETT.parse().unwrap();
    let old = Tools::new(api.clone(), mike.clone()).in_local(run.path());
    let m = connect(&api, old, &mike).await;
    let b = connect(&api, Tools::new(api.clone(), brett.clone()), &brett).await;
    let (left, error) = call(&m, "leave", json!({})).await;
    assert!(!error, "{left}");
    // The agent tool stops the old `riff mcp`, and starts a new one.
    drop(m);

    let tools = Tools::new(api.clone(), mike.clone()).in_local(run.path());
    assert!(tools.left());
    let m = connect_only(tools).await;
    let (text, error) = call(&m, "whoami", json!({})).await;
    assert!(error, "{text}");
    assert!(text.contains("/riff:join"), "{text}");
    let (who, _) = call(&b, "who", json!({})).await;
    assert!(!lists(&who, "a1"), "{who}");

    let fresh = Tools::new(api, brett).in_local(run.path());
    assert!(!fresh.left(), "a new session joins as usual");
}

/// With no directory for the mark, riff cannot keep a leave: the tool
/// refuses, and the session stays (01M3XQVK05FAT3PR43W8RNEYHY).
#[tokio::test]
async fn a_leave_that_riff_cannot_keep_is_refused() {
    let not_git = tempfile::tempdir().unwrap();
    let api = start_server().await;
    let mike: SessionUri = MIKE.parse().unwrap();
    let tools = Tools::new(api.clone(), mike.clone()).in_dir(not_git.path().to_owned());
    let m = connect(&api, tools, &mike).await;
    let (text, error) = call(&m, "leave", json!({})).await;
    assert!(error, "{text}");
    assert!(text.contains("still in the riff"), "{text}");
    let (who, error) = call(&m, "who", json!({})).await;
    assert!(!error, "{who}");
    assert!(lists(&who, "a1"), "{who}");
}
