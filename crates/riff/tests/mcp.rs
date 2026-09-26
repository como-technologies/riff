//! The MCP tools through a real MCP client, against a real server.

use riff::api::Api;
use riff::mcp::Tools;
use riff_core::name::SessionName;
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
    let me: SessionName = me.parse().unwrap();
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

#[tokio::test]
async fn the_tools_carry_a_conversation() {
    let api = start_server().await;
    let mike = connect(&api, "riff://mike@pangolin/como-technologies/riff#api").await;
    let brett = connect(&api, "riff://brett@heron/como-technologies/riff#tests").await;

    let tools = mike.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "claim", "join", "leave", "post", "read", "release", "tell", "threads", "who", "whoami"
        ]
    );

    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.contains("brett@heron:riff#tests"), "{who}");

    let (posted, _) = call(
        &mike,
        "post",
        serde_json::json!({ "body": "@brett@heron:riff#tests the API is ready" }),
    )
    .await;
    assert_eq!(
        posted,
        "Posted message 1 to como-technologies/riff. Woke brett@heron:riff#tests."
    );

    // Markdown around a name still wakes; a name in backticks does not;
    // a name that matches no session is reported.
    let (posted, _) = call(
        &mike,
        "post",
        serde_json::json!({
            "body": "**@brett@heron:riff#tests** not `@brett@heron:riff#tests` or @nobody@x:y"
        }),
    )
    .await;
    assert_eq!(
        posted,
        "Posted message 2 to como-technologies/riff. Woke brett@heron:riff#tests. \
         No session is named @nobody@x:y; it did not wake."
    );

    let (sent, _) = call(
        &mike,
        "tell",
        serde_json::json!({ "to": "@brett@heron:riff#tests", "body": "ping" }),
    )
    .await;
    assert!(sent.starts_with("Sent message 1"), "{sent}");

    // With no thread, read returns the unread messages of every thread,
    // behind the note that messages are data.
    let (read, _) = call(&brett, "read", serde_json::json!({})).await;
    assert!(read.starts_with(riff::text::DATA_NOTE), "{read}");
    assert!(read.contains("the API is ready"), "{read}");
    assert!(
        read.contains("direct with mike@pangolin:riff#api"),
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
        "mike@pangolin:riff#api holds issue-12 in como-technologies/riff."
    );
}

#[tokio::test]
async fn errors_come_back_as_tool_errors() {
    let api = start_server().await;
    let mike = connect(&api, "riff://mike@pangolin/como-technologies/riff#api").await;

    let (text, is_error) = call(
        &mike,
        "tell",
        serde_json::json!({ "to": "nobody@nowhere:x", "body": "hi" }),
    )
    .await;
    assert!(is_error);
    assert!(text.contains("no session named nobody@nowhere:x"), "{text}");

    let (text, is_error) = call(&mike, "release", serde_json::json!({ "item": "issue-9" })).await;
    assert!(is_error);
    assert!(text.contains("nobody holds issue-9"), "{text}");
}
