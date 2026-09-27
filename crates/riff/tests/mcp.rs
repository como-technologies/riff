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
// Each is the first session of its user, so each is its lead.
const MIKE_LEAD: &str = "riff://mike@pangolin/como-technologies/riff?session=a1&lead=true#api";
const BRETT_LEAD: &str = "riff://brett@heron/como-technologies/riff?session=b2&lead=true#tests";

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
            "claim", "join", "lead", "leave", "move", "post", "read", "release", "status", "tell",
            "threads", "who", "whoami"
        ]
    );

    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.contains(BRETT_LEAD), "{who}");
    assert!(who.contains("(b2) idle 0s  "), "{who}");
    let (all, _) = call(&mike, "who", serde_json::json!({ "all": true })).await;
    assert_eq!(all, who);

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
    // Without sign-in, no message is verified, so none shows as from the
    // lead (R200).
    let (read, _) = call(&brett, "read", serde_json::json!({})).await;
    assert!(read.starts_with(riff::text::DATA_NOTE), "{read}");
    assert!(
        read.contains(&format!(
            "{MIKE} to user=brett (not verified): the API is ready"
        )),
        "{read}"
    );
    assert!(!read.contains(MIKE_LEAD), "{read}");
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
async fn a_worker_asks_the_lead_and_gets_the_answer() {
    let api = start_server().await;
    let first = connect(&api, MIKE).await;
    let worker = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=c3#docs",
    )
    .await;
    let third = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=e5",
    )
    .await;

    // The first session is the lead with no action. The others are not.
    let (me, _) = call(&first, "whoami", serde_json::json!({})).await;
    assert!(me.ends_with(MIKE_LEAD), "{me}");
    for session in [&worker, &third] {
        let (me, _) = call(session, "whoami", serde_json::json!({})).await;
        assert!(!me.contains("lead=true"), "{me}");
    }

    // The worker asks the lead. The lead sends the answer back.
    let ask = serde_json::json!({ "session": "lead", "body": "merge now?" });
    let (sent, _) = call(&worker, "tell", ask).await;
    assert!(
        sent.ends_with("Woke mike@pangolin:riff#api (a1)."),
        "{sent}"
    );
    let (read, _) = call(&first, "read", serde_json::json!({})).await;
    assert!(
        read.contains("direct with mike@pangolin:riff#docs (c3)"),
        "{read}"
    );
    assert!(read.contains("merge now?"), "{read}");
    let answer = serde_json::json!({ "session": "c3", "body": "yes, merge" });
    call(&first, "tell", answer).await;
    let (read, _) = call(&worker, "read", serde_json::json!({})).await;
    assert!(read.contains("yes, merge"), "{read}");

    // The person marks another lead. It replaces the first.
    let (led, is_error) = call(&third, "lead", serde_json::json!({})).await;
    assert!(!is_error, "{led}");
    assert_eq!(
        led,
        "You are the lead of mike in como-technologies/riff. \
         mike@pangolin:riff#api (a1) is not the lead now."
    );
    let ask = serde_json::json!({ "session": "lead", "body": "and now?" });
    let (sent, _) = call(&worker, "tell", ask).await;
    assert!(sent.ends_with("Woke mike@pangolin:riff (e5)."), "{sent}");

    // The lead cannot ask itself. It asks its own user.
    let ask = serde_json::json!({ "session": "lead", "body": "me?" });
    let (text, is_error) = call(&third, "tell", ask).await;
    assert!(is_error);
    assert!(text.contains("Ask your own user"), "{text}");
}

#[tokio::test]
async fn a_status_request_gets_an_answer_with_the_status_tool() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    let brett = connect(&api, BRETT).await;

    let ask = serde_json::json!({
        "to": [{ "repo": "como-technologies/riff" }],
        "body": "",
        "kind": "status"
    });
    let (posted, _) = call(&mike, "post", ask).await;
    assert!(
        posted.ends_with("Woke brett@heron:riff#tests (b2)."),
        "{posted}"
    );

    let (read, _) = call(&brett, "read", serde_json::json!({})).await;
    assert!(
        read.contains(&format!(
            "{MIKE} to repo=como-technologies/riff (not verified) asks for your status."
        )),
        "{read}"
    );
    let answer = serde_json::json!({ "step": "merge", "blocked": "waits for a review" });
    let (set, is_error) = call(&brett, "status", answer).await;
    assert!(!is_error, "{set}");
    assert_eq!(
        set,
        "Your status is now: blocked at merge: waits for a review"
    );

    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(
        who.contains(&format!(
            "{BRETT_LEAD}\n  blocked 0s ago: waits for a review (step: merge)\n"
        )),
        "{who}"
    );

    let (text, is_error) = call(&brett, "status", serde_json::json!({ "step": "" })).await;
    assert!(is_error);
    assert!(text.contains("the step of a status is empty"), "{text}");
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
    assert_eq!(
        api.who(&MIKE.parse().unwrap(), false).await.unwrap().len(),
        1
    );

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
