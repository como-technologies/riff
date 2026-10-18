//! The MCP tools through a real MCP client, against a real server.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use isolated::{Isolated, Span, in_time};
use riff::mcp::Tools;
use riff_client::api::Api;
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

/// "Parts" in `how-it-works.md` names each tool of `riff mcp`, and no
/// other tool.
#[tokio::test]
async fn the_book_lists_each_tool() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    let mut tools: Vec<_> = mike
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    tools.sort();
    let book = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/how-it-works.md"),
    )
    .unwrap();
    let flat = book.split_whitespace().collect::<Vec<_>>().join(" ");
    let (_, rest) = flat
        .split_once("**`riff mcp`** gives your session its tools:")
        .unwrap();
    let (list, _) = rest.split_once('.').unwrap();
    let mut listed: Vec<_> = list
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    listed.sort();
    assert_eq!(listed, tools);
}

/// The kept answer for the status line of the session `a1` at `api`
/// in `dir`, once it holds `want`.
async fn kept_line(api: &Api, dir: &Path, want: impl Fn(&str) -> bool) -> String {
    let span = Span::start();
    loop {
        let now = riff::activity::now_ms();
        let keep = Duration::from_secs(90);
        if let Some(kept) = riff::local::kept_statusline(dir, "a1", api.base(), now, keep)
            && want(&kept.line)
        {
            return kept.line;
        }
        assert!(span.within(Duration::from_secs(20)), "no kept line");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 01M547VMMDPB4DXBMWVERF60C4: the tools keep the answer for the status
/// line after a claim and a release, and at a keep-alive.
#[tokio::test]
async fn the_tools_keep_the_answer_for_the_status_line() {
    let api = start_server().await;
    let state = tempfile::tempdir().unwrap();
    let me: SessionUri = MIKE.parse().unwrap();
    api.register(&me).await.unwrap();
    api.set_riff(&me, riff_core::wire::RiffState::Running)
        .await
        .unwrap();
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(api.clone(), me).with_statusline(Some(state.path().to_owned()));
    let alive = tools.clone();
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ().serve(client_io).await.unwrap();

    let (text, failed) = call(&client, "claim", serde_json::json!({"item": "issue-7"})).await;
    assert!(!failed, "{text}");
    let line = kept_line(&api, state.path(), |l| l.contains("issue-7")).await;
    assert_eq!(line, "lead issue-7");
    let (text, failed) = call(&client, "release", serde_json::json!({"item": "issue-7"})).await;
    assert!(!failed, "{text}");
    assert_eq!(
        kept_line(&api, state.path(), |l| !l.contains("issue-7")).await,
        "lead"
    );

    // The keep-alive keeps an answer too.
    std::fs::remove_dir_all(state.path()).unwrap();
    let task = alive.keep_alive_every(Duration::from_millis(20));
    assert_eq!(kept_line(&api, state.path(), |_| true).await, "lead");
    task.abort();
}

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
            "blocked",
            "claim",
            "free",
            "hold",
            "join",
            "join_thread",
            "lead",
            "leave",
            "leave_thread",
            "move",
            "pause",
            "post",
            "read",
            "release",
            "resume",
            "status",
            "tell",
            "threads",
            "who",
            "whoami"
        ]
    );

    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.contains(BRETT_LEAD), "{who}");
    // A lead with no watch is not offline (01M48VDGQ5KETKPM4G6TKTC2MB).
    assert!(who.contains("(b2) paused lead  "), "{who}");
    assert!(!who.contains("offline"), "{who}");
    // The who tool keeps plain text for agents (01M3Q63MVZ74WPNBA3QJYQGHFG).
    assert!(!who.contains('\x1b'), "{who:?}");
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
    // A riff with no sign-in trusts its network, so each message is
    // verified and shows the lead mark (R212).
    let (read, _) = call(&brett, "read", serde_json::json!({})).await;
    assert!(read.starts_with(riff::text::DATA_NOTE), "{read}");
    assert!(
        read.contains(
            "mike@pangolin:riff#api (a1) lead=true to user=brett (verified): the API is ready"
        ),
        "{read}"
    );
    assert!(
        read.contains("dm:a1 (direct with mike@pangolin:riff#api)"),
        "{read}"
    );
    assert!(read.contains("ping"), "{read}");
    let (again, _) = call(&brett, "read", serde_json::json!({})).await;
    assert_eq!(again, "No unread messages.");

    // A new riff is paused. Brett's session is the lead of brett, so it
    // can resume it; the state is one for the whole riff.
    let (refused, is_error) = call(&mike, "claim", serde_json::json!({ "item": "issue-12" })).await;
    assert!(is_error, "{refused}");
    assert!(refused.contains("the riff is paused"), "{refused}");
    let (me, _) = call(&mike, "whoami", serde_json::json!({})).await;
    assert!(
        me.contains(
            "The riff is paused by the server. Nobody claims work. The owner or an admin \
             resumes it with `riff resume --riff`.\n"
        ),
        "{me}"
    );
    assert!(me.ends_with(&riff::text::build_line(None)), "{me}");
    // With no argument, the tool names the repository of the lead.
    let (resumed, _) = call(&brett, "resume", serde_json::json!({})).await;
    assert_eq!(
        resumed,
        "The repository como-technologies/riff was running already. The whole riff is still \
         paused: the owner or an admin resumes it with `riff resume --riff`."
    );
    let (resumed, _) = call(&brett, "resume", serde_json::json!({ "riff": true })).await;
    assert_eq!(
        resumed,
        "The riff is running now. Woke mike@pangolin:riff#api (a1)."
    );
    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.starts_with("The riff is running.\n"), "{who}");

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
    assert!(me.contains(&format!("\n{MIKE_LEAD}\n")), "{me}");
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
        read.contains("dm:c3 (direct with mike@pangolin:riff#docs)"),
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

/// A tell of the lead to a new worker reaches it, before its first read
/// and between two pages of its start read (#651). The repository
/// thread is longer than one page. The first read with no thread and
/// `all` shows a tell that came before it. The next page, a read of all
/// after a seq of the repository thread, shows nothing of a tell that
/// came after the first page: it keeps the tell unread
/// (01M51TKZSYR7PH629S29JX00ZD), and the next read shows it.
#[tokio::test]
async fn a_tell_to_a_new_worker_stays_unread_until_a_read_shows_it() {
    use riff_state::state::PAGE;
    let api = start_server().await;
    let lead = connect(&api, MIKE).await;
    let lead_uri: SessionUri = MIKE.parse().unwrap();
    let repo = lead_uri.default_thread().unwrap();
    for n in 1..=PAGE + 10 {
        let body = format!("history {n}");
        api.post(&lead_uri, Some(&repo), &[], &body, Default::default())
            .await
            .unwrap();
    }
    let direct = "dm:a1 (direct with mike@pangolin:riff#api)";
    let more = format!(
        "More messages follow. Read the thread {repo} again. With all, set after to {PAGE}."
    );

    let w1 = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w1",
    )
    .await;
    let tell = serde_json::json!({ "session": "w1", "body": "request: claim issue-7" });
    call(&lead, "tell", tell).await;
    let (first, _) = call(&w1, "read", serde_json::json!({ "all": true })).await;
    assert!(first.contains(&more), "{first}");
    assert!(
        first.contains(&format!("{direct}\n[1] ")) && first.contains("request: claim issue-7"),
        "{first}"
    );

    let w2 = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w2",
    )
    .await;
    let (first, _) = call(&w2, "read", serde_json::json!({ "all": true })).await;
    assert!(first.contains(&more) && !first.contains(direct), "{first}");
    let tell = serde_json::json!({ "session": "w2", "body": "request: claim issue-12" });
    call(&lead, "tell", tell).await;
    let next = serde_json::json!({ "all": true, "after": PAGE });
    let (second, _) = call(&w2, "read", next).await;
    assert!(
        second.contains(&format!("history {}", PAGE + 10)),
        "{second}"
    );
    let (threads, _) = call(&w2, "threads", serde_json::json!({})).await;
    assert!(
        threads.contains(&format!("{direct}: 2 members, 1 unread")),
        "{threads}"
    );
    let (read, _) = call(&w2, "read", serde_json::json!({})).await;
    assert!(
        read.contains(&format!("{direct}\n[1] ")) && read.contains("request: claim issue-12"),
        "{read}"
    );
}

/// A session with 60 unread messages in its repository thread and one
/// unread direct message gets the direct message at the top of its
/// first read (01M58AG7Z2PETHBZG7X9QD8PK5). The name of a direct thread
/// sorts after the repository thread in the list of the server.
#[tokio::test]
async fn the_first_read_shows_the_direct_message_at_the_top() {
    let api = start_server().await;
    let lead = connect(&api, MIKE).await;
    let lead_uri: SessionUri = MIKE.parse().unwrap();
    let worker = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w1",
    )
    .await;
    let repo = lead_uri.default_thread().unwrap();
    for n in 1..=60 {
        let body = format!("history {n}");
        api.post(&lead_uri, Some(&repo), &[], &body, Default::default())
            .await
            .unwrap();
    }
    let tell = serde_json::json!({ "session": "w1", "body": "request: claim issue-7" });
    call(&lead, "tell", tell).await;

    let (read, _) = call(&worker, "read", serde_json::json!({})).await;
    let top = format!(
        "{}\n\ndm:a1 (direct with mike@pangolin:riff#api)\n[1] ",
        riff::text::DATA_NOTE
    );
    assert!(read.starts_with(&top), "{read}");
    assert!(read.contains("request: claim issue-7"), "{read}");
    assert!(read.contains("como-technologies/riff\n[1] "), "{read}");
}

/// A session reads its direct thread again by the name that `threads`
/// shows, after a read marked each message as read, and a long one page
/// by page with the step that its "More messages follow" line names
/// (#1111, 01M58AJE14M9TPTQE4E8R9PA9M). Another session cannot read it.
#[tokio::test]
async fn a_session_reads_its_direct_thread_again_by_its_name() {
    use riff_state::state::PAGE;
    let api = start_server().await;
    let lead = connect(&api, MIKE).await;
    let worker = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w1",
    )
    .await;
    for n in 1..=PAGE + 2 {
        let tell = serde_json::json!({ "session": "w1", "body": format!("request {n}") });
        call(&lead, "tell", tell).await;
    }
    loop {
        let (read, _) = call(&worker, "read", serde_json::json!({})).await;
        if read == "No unread messages." {
            break;
        }
    }

    let direct = "dm:a1 (direct with mike@pangolin:riff#api)";
    let (threads, _) = call(&worker, "threads", serde_json::json!({})).await;
    assert!(
        threads.contains(&format!("{direct}: 2 members, 0 unread")),
        "{threads}"
    );
    let more = format!(
        "More messages follow. Read the thread dm:a1 again. With all, set after to {PAGE}.\n"
    );
    for name in ["dm:a1", direct] {
        let args = serde_json::json!({ "thread": name, "all": true });
        let (page, is_error) = call(&worker, "read", args).await;
        assert!(!is_error, "{page}");
        assert!(page.contains(&format!("{direct}\n[1] ")), "{page}");
        assert!(
            page.contains(": request 1\n") && page.ends_with(&more),
            "{page}"
        );
    }
    let next = serde_json::json!({ "thread": "dm:a1", "all": true, "after": PAGE });
    let (page, is_error) = call(&worker, "read", next).await;
    assert!(!is_error, "{page}");
    assert!(
        page.contains(&format!(": request {}\n", PAGE + 2)) && !page.contains("More messages"),
        "{page}"
    );

    let brett = connect(&api, BRETT).await;
    let args = serde_json::json!({ "thread": "dm:a1", "all": true });
    let (refused, is_error) = call(&brett, "read", args).await;
    assert!(is_error, "{refused}");
    assert!(
        refused.contains("you have no direct thread dm:a1"),
        "{refused}"
    );
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
        read.contains(
            "mike@pangolin:riff#api (a1) lead=true to all (verified) asks for your status."
        ),
        "{read}"
    );
    let answer = serde_json::json!({ "step": "merge" });
    let (set, is_error) = call(&brett, "status", answer).await;
    assert!(!is_error, "{set}");
    assert_eq!(set, "Your status is now: merge");

    // With a watch, brett is live, so the server gives its state. The
    // riff is paused: paused wins, with the step it stopped at
    // (01M3QB6CJ1XCQG5B1BVR8AF3B4).
    let uri: riff_core::name::SessionUri = BRETT_LEAD.parse().unwrap();
    let _watch = api.watch(&uri).await.unwrap();
    let (who, _) = call(&mike, "who", serde_json::json!({})).await;
    assert!(who.contains("(b2) paused lead  "), "{who}");
    assert!(
        who.contains(&format!("{BRETT_LEAD}\n  stopped at: merge\n")),
        "{who}"
    );

    let (text, is_error) = call(&brett, "status", serde_json::json!({ "step": "" })).await;
    assert!(is_error);
    assert!(text.contains("the step is empty"), "{text}");
}

#[tokio::test]
async fn move_changes_the_place_and_keeps_the_claims() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    call(&mike, "resume", serde_json::json!({ "riff": true })).await;
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
    // The reply shows the claims, as the server has them
    // (01M517N7ANC06S4GTEEZHAB255).
    assert!(moved.contains("session=a1&claim=issue-6#notes"), "{moved}");
    let (me, _) = call(&mike, "whoami", serde_json::json!({})).await;
    assert!(me.contains("session=a1&claim=issue-6#notes"), "{me}");
    assert_eq!(
        api.who(&MIKE.parse().unwrap(), false).await.unwrap().len(),
        1
    );

    // A keep-alive and a status keep the claim (01M517N7FRG389BETVWA1SX5YD).
    api.alive(&MIKE.parse().unwrap()).await.unwrap();
    let (text, is_error) = call(&mike, "status", serde_json::json!({ "step": "tests" })).await;
    assert!(!is_error, "{text}");
    let who = api.who(&MIKE.parse().unwrap(), false).await.unwrap();
    assert_eq!(who[0].uri.claims(), ["issue-6"], "{:?}", who[0].uri);

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

/// A session in a container gets only the tools that talk. Each other
/// tool refuses and changes nothing: the claim and the place stay
/// (01M4ZV3B6T2KB09WJ59F07HXET).
#[tokio::test]
async fn a_session_in_a_container_gets_only_the_tools_that_talk() {
    let api = start_server().await;
    let host = connect(&api, MIKE).await;
    call(&host, "resume", serde_json::json!({ "riff": true })).await;
    let (claimed, _) = call(&host, "claim", serde_json::json!({ "item": "issue-12" })).await;
    assert_eq!(claimed, "You hold issue-12 in como-technologies/riff.");

    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(api.clone(), MIKE.parse().unwrap()).in_container();
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let mike = ().serve(client_io).await.unwrap();

    let mut listed: Vec<_> = mike
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    listed.sort();
    assert_eq!(listed, riff::mcp::CONTAINER_TOOLS);
    let info = mike.peer_info().unwrap();
    assert_eq!(
        info.instructions.as_deref(),
        Some(riff::text::MCP_CONTAINER)
    );

    let refused = [
        ("claim", serde_json::json!({ "item": "issue-13" })),
        ("release", serde_json::json!({ "item": "issue-12" })),
        ("leave", serde_json::json!({})),
        ("move", serde_json::json!({ "path": "/" })),
        ("lead", serde_json::json!({})),
        ("pause", serde_json::json!({})),
        (
            "hold",
            serde_json::json!({ "item": "issue-14", "reason": "r" }),
        ),
    ];
    for (tool, args) in refused {
        let (text, is_error) = call(&mike, tool, args).await;
        assert!(is_error, "{tool}: {text}");
        assert_eq!(text, riff::text::not_in_a_container(tool));
    }

    let (whoami, is_error) = call(&mike, "whoami", serde_json::json!({})).await;
    assert!(!is_error, "{whoami}");
    assert!(whoami.contains("claim=issue-12"), "{whoami}");
    assert!(!whoami.contains("issue-13"), "{whoami}");
    assert!(whoami.contains("claim=issue-12#api"), "{whoami}");
    let (posted, is_error) = call(&mike, "post", serde_json::json!({ "body": "hi" })).await;
    assert!(!is_error, "{posted}");
}

/// The MCP instructions state the rule of the read note
/// (01M3JEJW019FFEVQ0ZX17362EW).
#[tokio::test]
async fn the_instructions_say_how_to_act_on_a_message() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    let info = mike.peer_info().unwrap();
    let instructions = info.instructions.as_deref().unwrap();
    assert!(
        instructions.contains(riff::text::DATA_NOTE),
        "{instructions}"
    );
    assert!(instructions.contains("Talk to other sessions when it helps"));
}

/// Two sessions of one user that are not the lead talk about shared
/// work, with no lead (01M3JEJW26Y1C0RHM1CENJRDZ1).
#[tokio::test]
async fn two_workers_talk_with_no_lead() {
    let api = start_server().await;
    let _lead = connect(&api, MIKE).await;
    let one = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w1#one",
    )
    .await;
    let two = connect(
        &api,
        "riff://mike@pangolin/como-technologies/riff?session=w2#two",
    )
    .await;
    let ask = serde_json::json!({ "session": "w2", "body": "I edit hook.rs. Do you?" });
    let (sent, is_error) = call(&one, "tell", ask).await;
    assert!(!is_error, "{sent}");
    let (read, _) = call(&two, "read", serde_json::json!({})).await;
    assert!(read.contains("I edit hook.rs. Do you?"), "{read}");
    assert!(!read.contains("session=w1&lead=true"), "{read}");
    let answer = serde_json::json!({ "session": "w1", "body": "No. Go ahead." });
    let (sent, is_error) = call(&two, "tell", answer).await;
    assert!(!is_error, "{sent}");
    let (read, _) = call(&one, "read", serde_json::json!({})).await;
    assert!(read.contains("No. Go ahead."), "{read}");
}

/// Puts a copy of `from` at `to` as `cargo install` does: a new file
/// beside it, then a rename. A child process copies, so that no fork of
/// a parallel test holds a write fd of the file.
fn install(from: &Path, to: &Path) {
    let stage = to.with_extension("stage");
    let out = std::process::Command::new("cp")
        .arg(from)
        .arg(&stage)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    std::fs::rename(&stage, to).unwrap();
}

/// The inode of the binary that the process `pid` runs.
fn runs(pid: u32) -> u64 {
    std::fs::metadata(format!("/proc/{pid}/exe")).unwrap().ino()
}

/// A running `riff mcp` runs a new binary in place, and keeps the
/// connection: a tool call after the update answers from the new
/// binary, with no new initialize (01M3NT6WZTKAFKGDWGCFKC8TB5).
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_runs_the_new_binary_and_keeps_the_connection() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let stderr = dir.path().join("stderr");
    let mut cmd = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut child = cmd
        .arg("mcp")
        .current_dir(dir.path())
        .env("RIFF_HOME", dir.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    let old = runs(pid);

    install(&riff, &binary);
    let new = std::fs::metadata(&binary).unwrap().ino();
    assert_ne!(old, new);
    let span = Span::start();
    while runs(pid) != new && span.within(UPDATE_WAIT) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();
    assert_eq!(runs(pid), new, "{}", log());
    assert!(log().contains("a new riff is on disk"), "{}", log());

    // The same connection: no new initialize.
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    let (text, failed) = call(&client, "status", serde_json::json!({"step": "after"})).await;
    assert!(!failed, "{text}");
    assert!(child.try_wait().unwrap().is_none(), "{}", log());
}

/// Claude Code sends no initialize request: it starts with
/// `server/discover` (MCP `2026-07-28`), so rmcp knows no peer info.
/// `riff mcp` still runs each new binary at its next idle time, and
/// keeps the connection, also at the second update
/// (01M53QQR7J7DG86SMAERKMYSAF).
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_of_a_client_with_no_initialize_runs_each_new_binary() {
    use rmcp::model::ProtocolVersion;
    use rmcp::service::{ClientLifecycleMode, ClientServiceExt};

    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let stderr = dir.path().join("stderr");
    let mut cmd = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut child = cmd
        .arg("mcp")
        .current_dir(dir.path())
        .env("RIFF_HOME", dir.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let discover = ClientLifecycleMode::Discover {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
    };
    let client = ().serve_with_lifecycle(io, discover).await.unwrap();
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();

    for update in 1..=2 {
        install(&riff, &binary);
        let new = std::fs::metadata(&binary).unwrap().ino();
        let span = Span::start();
        while runs(pid) != new && span.within(UPDATE_WAIT) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(runs(pid), new, "update {update}: {}", log());
        // The same connection, with no new handshake: the build of the
        // new riff answers.
        let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
        assert!(!failed && text.contains("session=a1"), "{text}");
        let build = format!("have the build {}", riff_core::build::VERSION);
        assert!(text.contains(&build), "{text}");
    }
    assert_eq!(
        log().matches("a new riff is on disk").count(),
        2,
        "{}",
        log()
    );
    assert!(child.try_wait().unwrap().is_none(), "{}", log());
}

/// A `riff mcp` that runs a replaced riff for longer than
/// [`riff::binary::STUCK_AFTER`] cannot start again by itself. After a
/// tool, the hook of its session gives the session one line with the
/// fix, once for each new riff. A worker gets no line
/// (01M53QQRAJMB24B0804YHPMN18).
#[tokio::test(flavor = "multi_thread")]
async fn the_session_of_a_stuck_riff_mcp_gets_one_line_with_the_fix() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let binary = d.join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let input = r#"{"session_id":"x","tool_name":"Bash","tool_input":{"command":"ls","description":"List files"}}"#;
    std::fs::write(d.join("input"), input).unwrap();
    // The agent process: it starts `riff mcp` as its child, as Claude
    // Code does, and runs the hook of step N when `run-N` is there. Its
    // `riff mcp` makes no handshake, so it never runs a new riff.
    let agent = r#"
mkfifo "$D/in"
"$B" mcp < "$D/in" > /dev/null 2> "$D/mcp.err" &
exec 3> "$D/in"
n=1
while [ ! -e "$D/end" ]; do
  if [ -e "$D/run-$n" ]; then
    env $(cat "$D/run-$n") "$R" hook tool --done < "$D/input" > "$D/out-$n.part" 2>> "$D/hook.err"
    mv "$D/out-$n.part" "$D/out-$n"
    n=$((n + 1))
  fi
  sleep 0.05
done
"#;
    let mut cmd = tokio::process::Command::from(Isolated::shared().command("sh"));
    let mut child = cmd
        .args(["-c", agent])
        .current_dir(d)
        .env("D", d)
        .env("B", &binary)
        .env("R", &riff)
        .env("RIFF_HOME", d)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .env_remove("RIFF_WORKER")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let agent_pid = child.id().unwrap();
    let state = d.join("state");
    let span = Span::start();
    while riff::local::recorded(&state, agent_pid).is_none() && span.within(Duration::from_secs(20))
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = || {
        let read = |f: &str| std::fs::read_to_string(d.join(f)).unwrap_or_default();
        format!("mcp: {}\nhook: {}", read("mcp.err"), read("hook.err"))
    };
    assert!(
        riff::local::recorded(&state, agent_pid).is_some(),
        "{}",
        log()
    );
    let step = AtomicUsize::new(0);
    // Runs the hook in the agent with the variable `env`, and gives its
    // stdout.
    let hook = async |env: &str| {
        let n = step.fetch_add(1, Ordering::SeqCst) + 1;
        std::fs::write(d.join(format!("run-{n}")), env).unwrap();
        let out = d.join(format!("out-{n}"));
        let span = Span::start();
        while !out.exists() && span.within(Duration::from_secs(20)) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("step {n}: {e}"))
    };
    let old = |path: &Path| {
        let past = std::time::SystemTime::now() - riff::binary::STUCK_AFTER * 2;
        let epoch = past
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let out = std::process::Command::new("touch")
            .args(["-m", "-d", &format!("@{epoch}")])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };

    // The riff that riff mcp runs is on disk: no line.
    assert_eq!(hook("A=1").await, "", "{}", log());
    // A new riff that just came: riff mcp can still follow it.
    install(&riff, &binary);
    assert_eq!(hook("A=1").await, "", "{}", log());
    // The new riff came long ago: one line, then no more.
    old(&binary);
    let out = hook("A=1").await;
    let json: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    assert_eq!(json["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    let line = json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(line.contains("still run the old riff"), "{line}");
    assert!(line.contains("/mcp"), "{line}");
    assert!(line.contains(&binary.display().to_string()), "{line}");
    assert_eq!(hook("A=1").await, "", "{}", log());
    // A next new riff: a worker gets no line, the session gets one.
    install(&riff, &binary);
    old(&binary);
    assert_eq!(hook("RIFF_WORKER=1").await, "", "{}", log());
    assert!(
        hook("A=1").await.contains("still run the old riff"),
        "{}",
        log()
    );

    std::fs::write(d.join("end"), "").unwrap();
    let span = Span::start();
    while child.try_wait().unwrap().is_none() && span.within(Duration::from_secs(10)) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A riff-server that holds each register while `hold` is true. It
/// keeps the path of each call in `paths`.
async fn start_server_that_holds(
    hold: tokio::sync::watch::Receiver<bool>,
    paths: Arc<std::sync::Mutex<Vec<String>>>,
) -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let mut hold = hold.clone();
            let paths = paths.clone();
            async move {
                let path = request.uri().path().to_owned();
                paths.lock().unwrap().push(path.clone());
                if path == "/v1/register" {
                    let _ = hold.wait_for(|held| !*held).await;
                }
                next.run(request).await
            }
        },
    ));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

/// A worker that riff starts gets its riff tools also when riff-server
/// is slow: `riff mcp` answers the `server/discover` of a client of
/// `2026-07-28`, as Claude Code is, before its register ends. The tools
/// list at once, and a tool call waits for the register
/// (01M5512RG2TTTS85VY8CEFF8R4, #1013).
#[tokio::test(flavor = "multi_thread")]
async fn a_worker_gets_its_tools_while_its_register_waits() {
    use rmcp::model::ProtocolVersion;
    use rmcp::service::{ClientLifecycleMode, ClientServiceExt};

    let (hold, held) = tokio::sync::watch::channel(true);
    let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
    let api = start_server_that_holds(held, paths.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let stderr = dir.path().join("stderr");
    let mut cmd = tokio::process::Command::from(Isolated::shared().riff());
    let mut child = cmd
        .arg("mcp")
        .current_dir(dir.path())
        .env("RIFF_HOME", dir.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("RIFF_WORKER", "1")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let discover = ClientLifecycleMode::Discover {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
    };
    // Before the fix, the answer came only after the register.
    let Ok(client) = in_time(
        Duration::from_secs(20),
        ().serve_with_lifecycle(io, discover),
    )
    .await
    else {
        panic!("no answer while the register waits: {}", log());
    };
    let client = client.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "whoami"), "{tools:?}");
    assert!(tools.iter().any(|t| t.name == "move"), "{tools:?}");

    let peer = client.peer().clone();
    let whoami = tokio::spawn(async move {
        peer.call_tool(CallToolRequestParams::new("whoami".to_owned()))
            .await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!whoami.is_finished(), "the call did not wait: {}", log());
    let seen = paths.lock().unwrap().clone();
    assert!(seen.iter().all(|p| p == "/v1/register"), "{seen:?}");
    assert!(!seen.is_empty(), "no register: {}", log());

    hold.send_replace(false);
    let answer = in_time(Duration::from_secs(20), whoami)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = answer
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<String>();
    assert!(text.contains("session=a1"), "{text}");
    let seen = paths.lock().unwrap().clone();
    let first_other = seen.iter().position(|p| p != "/v1/register").unwrap();
    assert!(first_other > 0, "{seen:?}");
    assert!(child.try_wait().unwrap().is_none(), "{}", log());
}

/// When the tools refuse the `tools/list` of Claude Code, the session
/// has no riff tools. `riff mcp` keeps a mark for its agent process.
/// The PostToolUse hook gives the session one line, once for each
/// mark, and the SessionStart hook at each start. A list that works
/// removes the mark (01M5512RKABQ70QPGC48S5F67W, #1013).
#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_no_riff_tools_gets_a_line_with_the_commands() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let riff = Isolated::shared().riff_path();
    let tool = r#"{"session_id":"x","tool_name":"Bash","tool_input":{"command":"ls","description":"List files"}}"#;
    std::fs::write(d.join("tool"), tool).unwrap();
    std::fs::write(d.join("start"), r#"{"session_id":"a1","source":"clear"}"#).unwrap();
    // The agent process: it starts `riff mcp` as its child, as Claude
    // Code does, and runs the hook of step N when `run-N` is there.
    let agent = r#"
mkfifo "$D/in"
"$R" mcp < "$D/in" > "$D/mcp.out" 2> "$D/mcp.err" &
exec 3> "$D/in"
n=1
while [ ! -e "$D/end" ]; do
  if [ -e "$D/run-$n" ]; then
    case "$(cat "$D/run-$n")" in
      tool) "$R" hook tool --done < "$D/tool" > "$D/out-$n.part" 2>> "$D/hook.err" ;;
      start) "$R" hook session-start < "$D/start" > "$D/out-$n.part" 2>> "$D/hook.err" ;;
    esac
    mv "$D/out-$n.part" "$D/out-$n"
    n=$((n + 1))
  fi
  sleep 0.05
done
"#;
    let mut cmd = tokio::process::Command::from(Isolated::shared().command("sh"));
    let mut child = cmd
        .args(["-c", agent])
        .current_dir(d)
        .env("D", d)
        .env("R", &riff)
        .env("RIFF_HOME", d)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .env_remove("RIFF_WORKER")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let agent_pid = child.id().unwrap();
    let state = d.join("state");
    let log = || {
        let read = |f: &str| std::fs::read_to_string(d.join(f)).unwrap_or_default();
        format!(
            "out: {}\nmcp: {}\nhook: {}",
            read("mcp.out"),
            read("mcp.err"),
            read("hook.err")
        )
    };
    let span = Span::start();
    while riff::local::recorded(&state, agent_pid).is_none() && span.within(Duration::from_secs(20))
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        riff::local::recorded(&state, agent_pid).is_some(),
        "{}",
        log()
    );
    let mut to_mcp = std::fs::OpenOptions::new()
        .write(true)
        .open(d.join("in"))
        .unwrap();
    // Sends `line` to riff mcp, and gives its answer to the id `id`.
    let mut ask = async |line: &str, id: u32| {
        use std::io::Write;
        writeln!(to_mcp, "{line}").unwrap();
        let key = format!(r#""id":{id},"#);
        let span = Span::start();
        loop {
            let out = std::fs::read_to_string(d.join("mcp.out")).unwrap_or_default();
            if let Some(answer) = out.lines().find(|l| l.contains(&key)) {
                return answer.to_owned();
            }
            assert!(
                span.within(Duration::from_secs(20)),
                "no answer {id}: {}",
                log()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let meta = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}"#;
    let step = AtomicUsize::new(0);
    // Runs the hook `which` in the agent, and gives its stdout.
    let hook = async |which: &str| {
        let n = step.fetch_add(1, Ordering::SeqCst) + 1;
        std::fs::write(d.join(format!("run-{n}")), which).unwrap();
        let out = d.join(format!("out-{n}"));
        let span = Span::start();
        while !out.exists() && span.within(Duration::from_secs(30)) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        std::fs::read_to_string(&out).unwrap_or_else(|e| panic!("step {n}: {e}"))
    };
    let line = "riff: this session has no riff tools.";

    // The lines of Claude Code 2.1.302 after a late answer to its
    // probe: the probe, a legacy initialize, and a list with no _meta.
    let discover =
        format!(r#"{{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{{{meta}}}}}"#);
    let found = ask(&discover, 1).await;
    assert!(found.contains("2026-07-28"), "{found}");
    let initialize = r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"claude-code","version":"2.1.302"}}}"#;
    let initialized = ask(initialize, 2).await;
    assert!(initialized.contains("2025-11-25"), "{initialized}");
    let list = r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#;
    let refused = ask(list, 3).await;
    assert!(refused.contains("_meta is missing"), "{refused}");
    let span = Span::start();
    while riff::local::no_tools(&state, agent_pid).is_none() && span.within(Duration::from_secs(20))
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        riff::local::no_tools(&state, agent_pid).is_some(),
        "{}",
        log()
    );

    // After a tool: one line, then no more.
    let out = hook("tool").await;
    let json: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}: {}", log()));
    assert_eq!(json["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    let told = json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert_eq!(told, riff::text::no_tools(false));
    assert_eq!(hook("tool").await, "", "{}", log());
    // A new context gets it again.
    let start = hook("start").await;
    assert!(start.contains(&format!("- {line}")), "{start}");
    assert_eq!(hook("tool").await, "", "{}", log());

    // A list that works removes the mark.
    let listed = ask(
        &format!(r#"{{"jsonrpc":"2.0","id":4,"method":"tools/list","params":{{{meta}}}}}"#),
        4,
    )
    .await;
    assert!(listed.contains("whoami"), "{listed}");
    let span = Span::start();
    while riff::local::no_tools(&state, agent_pid).is_some() && span.within(Duration::from_secs(20))
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(riff::local::no_tools(&state, agent_pid), None, "{}", log());
    assert!(!hook("start").await.contains(line), "{}", log());

    std::fs::write(d.join("end"), "").unwrap();
    let span = Span::start();
    while child.try_wait().unwrap().is_none() && span.within(Duration::from_secs(10)) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A new binary that fails its check does not take the place of a
/// running `riff mcp`: the old process keeps the tools, says the error
/// once, and runs the next new binary that passes
/// (01M43F5F9AQ9S39E1JZF8EBJEH).
#[tokio::test(flavor = "multi_thread")]
async fn a_new_riff_that_fails_its_check_leaves_the_tools_in_place() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let broken = dir.path().join("broken");
    isolated::write_program(
        &broken,
        "#!/bin/sh\necho 'riff: cannot read the sign-in of this machine' >&2\nexit 1\n",
    );
    let stderr = dir.path().join("stderr");
    let mut cmd = tokio::process::Command::from(Isolated::shared().command(&binary));
    let mut child = cmd
        .arg("mcp")
        .current_dir(dir.path())
        .env("RIFF_HOME", dir.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
    let client = ().serve(io).await.unwrap();
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    let old = runs(pid);
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();

    install(&broken, &binary);
    let span = Span::start();
    while !log().contains("fails its check") && span.within(Duration::from_secs(15)) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        log().contains("cannot read the sign-in of this machine"),
        "{}",
        log()
    );
    assert_eq!(runs(pid), old, "{}", log());
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    // The error comes once, not at each look at the binary.
    tokio::time::sleep(riff::binary::POLL * 3).await;
    assert_eq!(log().matches("fails its check").count(), 1, "{}", log());

    install(&riff, &binary);
    let new = std::fs::metadata(&binary).unwrap().ino();
    let span = Span::start();
    while runs(pid) != new && span.within(Duration::from_secs(15)) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(runs(pid), new, "{}", log());
    let (text, failed) = call(&client, "whoami", serde_json::json!({})).await;
    assert!(!failed && text.contains("session=a1"), "{text}");
    assert!(child.try_wait().unwrap().is_none(), "{}", log());
}

/// `riff mcp --check` with the client of a session does each step of
/// the start that can fail, serves nothing and ends. A client that it
/// cannot read fails it (01M43F5F9AQ9S39E1JZF8EBJEH).
#[tokio::test(flavor = "multi_thread")]
async fn the_check_of_riff_mcp_serves_nothing() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let check = |client: &str| {
        let mut cmd = Isolated::shared().riff();
        cmd.args(["mcp", "--client", client, "--check"])
            .current_dir(dir.path())
            .env("RIFF_HOME", dir.path())
            .env("RIFF_USER", "mike")
            .env("RIFF_HOST", "pangolin")
            .env("RIFF_SESSION", "a1")
            .env("RIFF_SERVER", api.base())
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("TMUX")
            .stdin(Stdio::null());
        cmd
    };
    let client = r#"{"protocolVersion":"2025-06-18","capabilities":{},
        "clientInfo":{"name":"claude-code","version":"2"}}"#;
    let mut good = check(client);
    let out = tokio::task::spawn_blocking(move || good.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    // A client with no initialize request (01M53QQR7J7DG86SMAERKMYSAF).
    let mut modern = check(riff::mcp::MODERN);
    let out = tokio::task::spawn_blocking(move || modern.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let mut bad = check("{}");
    let out = tokio::task::spawn_blocking(move || bad.output().unwrap())
        .await
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
}

/// `riff mcp` with a file as stdin stops with an error that says so.
#[tokio::test(flavor = "multi_thread")]
async fn riff_mcp_names_a_file_as_stdin() {
    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::write(&input, "").unwrap();
    let mut cmd = Isolated::shared().riff();
    cmd.arg("mcp")
        .current_dir(dir.path())
        .env("RIFF_HOME", dir.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", api.base())
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .stdin(std::fs::File::open(&input).unwrap());
    // Away from the runtime of the server.
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert!(
        err.contains("reads stdin from a pipe or a terminal, not from a file"),
        "{err}"
    );
}

/// The stdout of `riff mcp`, line by line.
type McpLines = tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>;

/// Starts `binary mcp` for the session a1 at the riff `url`, with its
/// stderr in `stderr`, and makes the handshake as Claude Code does.
async fn raw_mcp(
    binary: &Path,
    url: &str,
    dir: &Path,
    stderr: &Path,
) -> (tokio::process::Child, tokio::process::ChildStdin, McpLines) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut cmd = tokio::process::Command::from(Isolated::shared().command(binary));
    let mut child = cmd
        .arg("mcp")
        .current_dir(dir)
        .env("RIFF_HOME", dir)
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SESSION", "a1")
        .env("RIFF_SERVER", url)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("TMUX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(stderr).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let init = serde_json::json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}}});
    stdin
        .write_all(format!("{init}\n").as_bytes())
        .await
        .unwrap();
    let first = lines.next_line().await.unwrap().unwrap();
    assert!(first.contains("\"id\":0"), "{first}");
    let initialized = "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
    stdin.write_all(initialized.as_bytes()).await.unwrap();
    (child, stdin, lines)
}

/// A tool call of JSON-RPC `id` on one line.
fn call_line(id: u64, tool: &str) -> String {
    let call = serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": tool, "arguments": {}}});
    format!("{call}\n")
}

/// Calls stream in bursts while a new binary comes: `riff mcp` runs it
/// only at a moment with no request in flight, so each call gets
/// exactly one answer (01M3NT6WZTKAFKGDWGCFKC8TB5).
#[tokio::test(flavor = "multi_thread")]
async fn each_call_in_flight_at_an_update_gets_one_answer() {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::io::AsyncWriteExt;

    let api = start_server().await;
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let stderr = dir.path().join("stderr");
    let (child, mut stdin, mut lines) = raw_mcp(&binary, api.base(), dir.path(), &stderr).await;
    let pid = child.id().unwrap();
    let answers: Arc<Mutex<HashMap<u64, usize>>> = Arc::default();
    let seen = answers.clone();
    let reader = tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            let answer: serde_json::Value = serde_json::from_str(&line).unwrap();
            if let Some(id) = answer.get("id").and_then(serde_json::Value::as_u64) {
                *seen.lock().unwrap().entry(id).or_default() += 1;
            }
        }
    });

    // Bursts of calls for 3 seconds. The new binary comes after half a
    // second, so calls are in flight when riff mcp sees it.
    let mut last = 0;
    let start = Instant::now();
    let mut installed = false;
    while start.elapsed() < Duration::from_secs(3) {
        if !installed && start.elapsed() > Duration::from_millis(500) {
            install(&riff, &binary);
            installed = true;
        }
        let mut burst = String::new();
        for tool in ["whoami", "who", "whoami"] {
            last += 1;
            burst.push_str(&call_line(last, tool));
        }
        stdin.write_all(burst.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(7)).await;
    }
    let new = std::fs::metadata(&binary).unwrap().ino();
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();
    let span = Span::start();
    while runs(pid) != new && span.within(UPDATE_WAIT) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(runs(pid), new, "{}", log());
    last += 1;
    stdin
        .write_all(call_line(last, "whoami").as_bytes())
        .await
        .unwrap();

    let span = Span::start();
    while answers.lock().unwrap().len() < last as usize && span.within(Duration::from_secs(20)) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let answers = answers.lock().unwrap();
    let missing: Vec<u64> = (1..=last).filter(|id| !answers.contains_key(id)).collect();
    let twice: Vec<u64> = (1..=last)
        .filter(|id| answers.get(id).is_some_and(|n| *n > 1))
        .collect();
    assert!(missing.is_empty(), "missing {missing:?}\n{}", log());
    assert!(twice.is_empty(), "twice {twice:?}");
    assert_eq!(
        log().matches("a new riff is on disk").count(),
        1,
        "{}",
        log()
    );
    reader.abort();
}

/// While `on`, the riff holds each request for [`HOLD`], and counts it.
#[derive(Clone, Default)]
struct Hold {
    on: Arc<AtomicBool>,
    held: Arc<AtomicUsize>,
}

/// Longer than `riff mcp` takes to see a new binary on disk.
const HOLD: Duration = Duration::from_secs(5);

/// The longest wait for the run of a new binary and its first answer:
/// the check of the new binary ([`riff::mcp::CHECK_WAIT`]), the exec and
/// the start of the new `riff mcp` (01M41A0M2XWCWTWGF7T9DR03W0).
const UPDATE_WAIT: Duration = riff::mcp::CHECK_WAIT.saturating_add(Duration::from_secs(10));

async fn hold(
    axum::extract::State(hold): axum::extract::State<Hold>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if hold.on.load(Ordering::SeqCst) {
        hold.held.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(HOLD).await;
    }
    next.run(request).await
}

/// A tool call is in flight when the new binary comes: the riff holds
/// its answer. `riff mcp` runs the new binary only after the answer, so
/// the call gets its answer, once (01M3NT6WZTKAFKGDWGCFKC8TB5).
#[tokio::test(flavor = "multi_thread")]
async fn a_call_in_flight_at_an_update_gets_its_answer() {
    use tokio::io::AsyncWriteExt;

    let held = Hold::default();
    let router =
        riff_server::router().layer(axum::middleware::from_fn_with_state(held.clone(), hold));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("riff");
    let riff = Isolated::shared().riff_path();
    install(&riff, &binary);
    let stderr = dir.path().join("stderr");
    let (mut child, mut stdin, mut lines) = raw_mcp(&binary, &url, dir.path(), &stderr).await;
    let pid = child.id().unwrap();
    let log = || std::fs::read_to_string(&stderr).unwrap_or_default();

    // The call waits at the riff while the new binary comes.
    held.on.store(true, Ordering::SeqCst);
    stdin
        .write_all(call_line(1, "who").as_bytes())
        .await
        .unwrap();
    let span = Span::start();
    while held.held.load(Ordering::SeqCst) == 0 {
        assert!(span.within(Duration::from_secs(10)), "{}", log());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    install(&riff, &binary);
    let new = std::fs::metadata(&binary).unwrap().ino();

    // Its answer comes.
    let answer = in_time(HOLD * 3, lines.next_line()).await;
    let answer = answer.ok().and_then(|line| line.unwrap());
    let answer = answer.unwrap_or_else(|| panic!("no answer to the call: {}", log()));
    held.on.store(false, Ordering::SeqCst);
    assert!(answer.contains("\"id\":1"), "{answer}");
    assert!(answer.contains("session=a1"), "{answer}");

    // Then the new binary runs, and the next answer is of the next call.
    let span = Span::start();
    while runs(pid) != new && span.within(UPDATE_WAIT) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(runs(pid), new, "{}", log());
    stdin
        .write_all(call_line(2, "whoami").as_bytes())
        .await
        .unwrap();
    let next = in_time(UPDATE_WAIT, lines.next_line()).await;
    let next = next.ok().and_then(|line| line.unwrap()).unwrap_or_default();
    assert!(next.contains("\"id\":2"), "{next}");
    assert!(child.try_wait().unwrap().is_none(), "{}", log());
}

/// The case of #498 through the tools (01M43GSGB9ZFHSG0Q83Y50FEGW,
/// 01M43GSGPJ69TPWPA4935WR8RW, 01M43GSGGY0QMB5D5EH92M6ZFP): the lead
/// holds `issue-12`; a worker gets `on_hold` with the lead, the time and
/// the reason, and cannot hold or free; brett gets the item with a
/// warning; after `free`, the worker gets the item.
#[tokio::test]
async fn the_lead_holds_an_item_and_a_worker_cannot_claim_it() {
    let api = start_server().await;
    let mike = connect(&api, MIKE).await;
    call(&mike, "resume", serde_json::json!({ "riff": true })).await;
    let reason = "waits for the word of Mike";
    let hold = serde_json::json!({ "item": "issue-12", "reason": reason });
    let (held, is_error) = call(&mike, "hold", hold.clone()).await;
    assert!(!is_error, "{held}");
    assert!(
        held.starts_with("issue-12 in como-technologies/riff is on hold now"),
        "{held}"
    );
    let (again, _) = call(&mike, "hold", hold).await;
    assert!(
        again.contains("on hold with this reason already"),
        "{again}"
    );

    let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1#w1"
        .parse()
        .unwrap();
    api.register_as(&worker, true).await.unwrap();
    let thread = worker.default_thread().unwrap();
    let refused = api.claim(&worker, &thread, "issue-12").await.unwrap();
    assert!(!refused.granted);
    let text = refused.held.unwrap();
    assert!(
        text.starts_with("issue-12 is held by the lead (the session mike/a1) since 20"),
        "{text}"
    );
    assert!(
        text.ends_with(&format!("{reason}. Pick another item.")),
        "{text}"
    );
    let error = api.hold(&worker, &thread, "issue-13", "mine").await;
    let error = format!("{:#}", error.unwrap_err());
    assert!(error.contains("a worker cannot hold an item"), "{error}");
    let error = api.free(&worker, &thread, "issue-12").await;
    let error = format!("{:#}", error.unwrap_err());
    assert!(error.contains("a worker cannot free an item"), "{error}");

    let brett = connect(&api, BRETT).await;
    let claim = serde_json::json!({ "item": "issue-12" });
    let (granted, is_error) = call(&brett, "claim", claim.clone()).await;
    assert!(!is_error, "{granted}");
    let warned =
        "You hold issue-12 in como-technologies/riff. Warning: issue-12 is held by the lead";
    assert!(granted.starts_with(warned), "{granted}");
    assert!(
        granted.contains("A worker does not get this claim."),
        "{granted}"
    );
    call(&brett, "release", claim).await;

    let free = serde_json::json!({ "item": "issue-12" });
    let (freed, _) = call(&mike, "free", free.clone()).await;
    assert!(freed.contains("is free of its hold"), "{freed}");
    let (twice, _) = call(&mike, "free", free).await;
    assert!(twice.contains("was not on hold"), "{twice}");
    let claimed = api.claim(&worker, &thread, "issue-12").await.unwrap();
    assert!(claimed.granted && claimed.warning.is_none());
}
