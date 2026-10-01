//! The automatic step of the lead (01M3W8AYDFPZNZ898WAJS7JEZA): the MCP
//! tools of the lead set its step from its `tell`, `post`, `pause`,
//! `resume` and `lead` calls, and `riff who` shows it. Through a real
//! MCP client and the `riff` binary, against a real server.

use std::path::Path;
use std::process::Command as Git;

use futures::StreamExt;
use isolated::Isolated;
use riff::api::Api;
use riff::mcp::Tools;
use riff_core::name::SessionUri;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};

async fn start_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    format!("http://{addr}")
}

/// A git repository with a GitHub origin, so `riff who` runs in
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
        let git = Git::new("git").args(args).current_dir(dir.path()).status();
        assert!(git.unwrap().success());
    }
    dir
}

fn uri(id: &str) -> SessionUri {
    format!("riff://mike@pangolin/como-technologies/riff?session={id}")
        .parse()
        .unwrap()
}

/// Connects an MCP client to the tools of the session `id` of mike.
async fn connect(api: &Api, id: &str) -> RunningService<RoleClient, ()> {
    let me = uri(id);
    api.register(&me).await.unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(api.clone(), me);
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

/// Calls a tool. The call must not fail.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str, args: serde_json::Value) {
    let params = CallToolRequestParams::new(tool.to_owned())
        .with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(params).await.unwrap();
    assert_ne!(result.is_error, Some(true), "{tool}: {result:?}");
}

/// The row of the session `id` in `riff who`, as the person mike sees
/// it in a shell.
async fn who_row(server: &str, dir: &Path, id: &str) -> String {
    let mut cmd = Isolated::shared().assert_riff();
    cmd.arg("who")
        .current_dir(dir)
        .env("RIFF_SERVER", server)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env_remove("RIFF_SESSION")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env("RIFF_HOME", dir);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = String::from_utf8(out.stdout).unwrap();
    let row = out.lines().find(|l| l.contains(&format!("({id})")));
    row.unwrap_or_else(|| panic!("no row of {id}: {out}"))
        .to_owned()
}

/// A running riff with the lead `a1` and the session `b2`, each live.
struct Riff {
    server: String,
    dir: tempfile::TempDir,
    lead: RunningService<RoleClient, ()>,
    other: RunningService<RoleClient, ()>,
}

/// Keeps a watch of the session `id` open. A session with a watch is
/// live, so `who` shows its step. It returns when the watch is open.
async fn watch(api: &Api, id: &str) {
    let (api, me) = (api.clone(), uri(id));
    let (open, opened) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let wakes = api.watch(&me).await.unwrap();
        open.send(()).unwrap();
        wakes.for_each(|_| async {}).await;
    });
    opened.await.unwrap();
}

impl Riff {
    async fn start() -> Riff {
        let server = start_server().await;
        let api = Api::new(&server);
        // The first session of mike is the lead.
        let lead = connect(&api, "a1").await;
        let other = connect(&api, "b2").await;
        // A paused riff hides the current step.
        call(&lead, "resume", serde_json::json!({})).await;
        watch(&api, "a1").await;
        watch(&api, "b2").await;
        Riff {
            server,
            dir: repo(),
            lead,
            other,
        }
    }

    async fn row(&self, id: &str) -> String {
        who_row(&self.server, self.dir.path(), id).await
    }
}

#[tokio::test]
async fn who_shows_the_tell_of_the_lead_as_its_step() {
    let riff = Riff::start().await;
    let row = riff.row("a1").await;
    assert!(row.ends_with(" ago: resumed the riff"), "{row}");

    let tell = serde_json::json!({ "session": "b2", "body": "request: claim issue-302" });
    call(&riff.lead, "tell", tell).await;
    let row = riff.row("a1").await;
    assert!(
        row.ends_with(" ago: told b2: request: claim issue-302"),
        "{row}"
    );

    // A step is one line.
    let note = serde_json::json!({
        "body": "Waves:\n  new item #314",
        "kind": "note",
        "to": [{ "repo": "como-technologies/riff" }]
    });
    call(&riff.lead, "post", note).await;
    let row = riff.row("a1").await;
    assert!(
        row.ends_with(" ago: posted a note: Waves: new item #314"),
        "{row}"
    );

    call(&riff.lead, "pause", serde_json::json!({})).await;
    let row = riff.row("a1").await;
    assert!(row.ends_with("stopped at: paused the riff"), "{row}");
}

#[tokio::test]
async fn a_tell_of_a_session_that_is_not_the_lead_keeps_its_step() {
    let riff = Riff::start().await;
    // With no step, the session gets none.
    let before = riff.row("b2").await;
    assert!(!before.contains(" ago: "), "{before}");
    let ask = serde_json::json!({ "session": "lead", "body": "merge now?" });
    call(&riff.other, "tell", ask.clone()).await;
    let row = riff.row("b2").await;
    assert!(!row.contains("told"), "{row}");
    assert!(!row.contains(" ago: "), "{row}");

    // With a step, the session keeps it.
    let step = serde_json::json!({ "step": "write the tests" });
    call(&riff.other, "status", step).await;
    call(&riff.other, "tell", ask).await;
    let note = serde_json::json!({ "body": "started", "kind": "note" });
    call(&riff.other, "post", note).await;
    let row = riff.row("b2").await;
    assert!(row.ends_with(" ago: write the tests"), "{row}");
}

#[tokio::test]
async fn a_status_call_of_the_lead_replaces_the_automatic_step() {
    let riff = Riff::start().await;
    let tell = serde_json::json!({ "session": "b2", "body": "request: claim issue-302" });
    call(&riff.lead, "tell", tell.clone()).await;
    let row = riff.row("a1").await;
    assert!(row.contains("told b2: request: claim issue-302"), "{row}");

    let step = serde_json::json!({ "step": "read the review report" });
    call(&riff.lead, "status", step).await;
    let row = riff.row("a1").await;
    assert!(row.ends_with(" ago: read the review report"), "{row}");
    assert!(!row.contains("told"), "{row}");

    // The step of the lead wins only until its next call.
    call(&riff.lead, "tell", tell).await;
    let row = riff.row("a1").await;
    assert!(
        row.ends_with(" ago: told b2: request: claim issue-302"),
        "{row}"
    );
}

#[tokio::test]
async fn a_new_lead_shows_that_it_became_the_lead() {
    let riff = Riff::start().await;
    call(&riff.other, "lead", serde_json::json!({})).await;
    let row = riff.row("b2").await;
    assert!(row.ends_with(" ago: became the lead"), "{row}");
}
