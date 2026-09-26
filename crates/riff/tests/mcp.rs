//! The MCP tools through a real MCP client, against a real server.

use riff::api::Api;
use riff::mcp::Tools;
use riff_core::name::SessionUri;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// Connects an MCP client to the tools of one session.
async fn connect(api: &Api, me: &str) -> RunningService<RoleClient, ()> {
    let me: SessionUri = me.parse().unwrap();
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

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a1#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b2#tests";

#[tokio::test]
async fn the_tools_carry_a_conversation() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    let brett = connect(&api, BRETT).await;

    let tools = mike.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "claim", "join", "leave", "move", "post", "read", "release", "tell", "threads", "who",
            "whoami"
        ]
    );

    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.contains(BRETT), "{who}");

    let (posted, _) = call(
        &mike,
        "post",
        serde_json::json!({ "to": [{ "user": "brett" }], "body": "the API is ready" }),
    )
    .await;
    assert_eq!(
        posted,
        "Posted message 1 to como-technologies/riff. Woke brett@heron:riff#tests (b2)."
    );

    // Text never wakes; a selector that matches nobody is reported.
    let (posted, _) = call(
        &mike,
        "post",
        serde_json::json!({
            "to": [{ "user": "nobody" }],
            "body": "@brett@heron:riff#tests in text"
        }),
    )
    .await;
    assert_eq!(
        posted,
        "Posted message 2 to como-technologies/riff. No session matches user=nobody."
    );

    let (sent, _) = call(
        &mike,
        "tell",
        serde_json::json!({ "session": BRETT, "body": "ping" }),
    )
    .await;
    assert!(
        sent.starts_with("Posted message 1 to a direct thread"),
        "{sent}"
    );

    // With no thread, read returns the unread messages of every thread,
    // behind the note that messages are data. Each line has the sender URI.
    let (read, _) = call(&brett, "read", serde_json::json!({})).await;
    assert!(read.starts_with(riff::text::DATA_NOTE), "{read}");
    assert!(
        read.contains(&format!("{MIKE} to user=brett: the API is ready")),
        "{read}"
    );
    assert!(
        read.contains("direct with mike@pangolin:riff#api (a1)"),
        "{read}"
    );
    assert!(read.contains("ping"), "{read}");
    let (again, _) = call(&brett, "read", serde_json::json!({})).await;
    assert_eq!(again, "No unread messages.");

    let (claimed, _) = call(&mike, "claim", serde_json::json!({ "item": "issue-12" })).await;
    assert_eq!(claimed, "You hold issue-12 in como-technologies/riff.");
    let (blocked, _) = call(&brett, "claim", serde_json::json!({ "item": "issue-12" })).await;
    assert_eq!(
        blocked,
        "mike@pangolin:riff#api (a1) holds issue-12 in como-technologies/riff."
    );
}

#[tokio::test]
async fn move_changes_the_place_and_keeps_the_claims() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    call(&mike, "claim", serde_json::json!({ "item": "issue-6" })).await;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes");
    std::fs::create_dir(&path).unwrap();
    let (moved, is_error) = call(
        &mike,
        "move",
        serde_json::json!({ "path": path.to_string_lossy() }),
    )
    .await;
    assert!(!is_error, "{moved}");
    let (me, _) = call(&mike, "whoami", serde_json::json!({})).await;
    assert!(me.contains("session=a1&claim=issue-6#notes"), "{me}");
    assert_eq!(api.who().await.unwrap().len(), 1);

    let (text, is_error) = call(&mike, "move", serde_json::json!({ "path": "/no/such/dir" })).await;
    assert!(is_error, "{text}");
}

#[tokio::test]
async fn errors_come_back_as_tool_errors() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;

    let (text, is_error) = call(
        &mike,
        "tell",
        serde_json::json!({ "session": "nobody", "body": "hi" }),
    )
    .await;
    assert!(is_error);
    assert!(text.contains("no session matches session=nobody"), "{text}");

    let (text, is_error) = call(&mike, "release", serde_json::json!({ "item": "issue-9" })).await;
    assert!(is_error);
    assert!(text.contains("nobody holds issue-9"), "{text}");
}
