//! The MCP tools through a real MCP client, against a real server.

use riff::tools::Tools;
use riff_client::api::Api;
use riff_core::name::SessionUri;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};

async fn start_server() -> crate::common::Signed {
    crate::common::start_signed().await
}

/// A client of the session `me` (or of the person, when `me` has no session).
fn of(server: &crate::common::Signed, me: impl std::fmt::Display) -> Api {
    crate::common::api_of(server, &me.to_string().parse().unwrap())
}

/// Connects an MCP client to the tools of `me`. The person names [`MIKE`]
/// and [`BRETT`] the lead.
async fn connect(server: &crate::common::Signed, me: &str) -> RunningService<RoleClient, ()> {
    let me: SessionUri = me.parse().unwrap();
    let api = of(server, &me);
    api.register(&me).await.unwrap();
    if [MIKE, BRETT].contains(&me.to_string().as_str()) {
        crate::common::name_lead(&server.api(me.who().user(), None), &me).await;
    }
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(api, me);
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
// The person of each user names it the lead in [`connect`].
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
            "blocked",
            "claim",
            "free",
            "hold",
            "join",
            "join_thread",
            "leave",
            "leave_thread",
            "move",
            "notes",
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

    // The person named the first session the lead. The others are not.
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

    // The person names another lead. It replaces the first.
    let e5: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=e5"
        .parse()
        .unwrap();
    crate::common::name_lead(&api.api("mike", None), &e5).await;
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
        of(&api, &lead_uri)
            .post(&lead_uri, Some(&repo), &[], &body, Default::default())
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
        of(&api, &lead_uri)
            .post(&lead_uri, Some(&repo), &[], &body, Default::default())
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
    let watcher = of(&api, &uri);
    let _watch = watcher.watch(&uri).await.unwrap();
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
        of(&api, MIKE)
            .who(&MIKE.parse().unwrap(), false)
            .await
            .unwrap()
            .len(),
        1
    );

    // A keep-alive and a status keep the claim (01M517N7FRG389BETVWA1SX5YD).
    of(&api, MIKE).alive(&MIKE.parse().unwrap()).await.unwrap();
    let (text, is_error) = call(&mike, "status", serde_json::json!({ "step": "tests" })).await;
    assert!(!is_error, "{text}");
    let who = of(&api, MIKE)
        .who(&MIKE.parse().unwrap(), false)
        .await
        .unwrap();
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
    let tools = Tools::new(of(&api, MIKE), MIKE.parse().unwrap()).in_container();
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
    assert_eq!(listed, riff::tools::CONTAINER_TOOLS);
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
        ("pause", serde_json::json!({})),
        (
            "hold",
            serde_json::json!({ "item": "issue-14", "reason": "r" }),
        ),
        ("notes", serde_json::json!({ "text": "x" })),
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
    of(&api, &worker).register_as(&worker, true).await.unwrap();
    let thread = worker.default_thread().unwrap();
    let refused = of(&api, &worker)
        .claim(&worker, &thread, "issue-12")
        .await
        .unwrap();
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
    let error = of(&api, &worker)
        .hold(&worker, &thread, "issue-13", "mine")
        .await;
    let error = format!("{:#}", error.unwrap_err());
    assert!(error.contains("a worker cannot hold an item"), "{error}");
    let error = of(&api, &worker).free(&worker, &thread, "issue-12").await;
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
    let claimed = of(&api, &worker)
        .claim(&worker, &thread, "issue-12")
        .await
        .unwrap();
    assert!(claimed.granted && claimed.warning.is_none());
}

/// The case of #1164 (01M59258K3TYR09HC4AWHWZPEX,
/// 01M59258NTXPMWV18TKRKWNTYN, 01M59258RJ3CG498Z8AVQ5TV5Q,
/// 01M592590XREH99ABQF2D2KT6H): a lead on a host with no Claude memory
/// runs from riff alone. Its start text names the notes, the instructions
/// carry its rules, and the `notes` tool keeps the state, with the
/// history. A text over the limit and a text with a token are refused.
/// A worker has no notes tool.
#[tokio::test]
async fn a_lead_with_no_memory_gets_its_state_from_the_notes() {
    let api = start_server().await;
    let host = connect(&api, MIKE).await;
    call(&host, "resume", serde_json::json!({ "riff": true })).await;

    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let tools = Tools::new(of(&api, MIKE), MIKE.parse().unwrap()).in_lead_container();
    tokio::spawn(async move {
        tools
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let lead = ().serve(client_io).await.unwrap();

    let mut listed: Vec<_> = lead
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    listed.sort();
    assert_eq!(listed, riff::tools::LEAD_TOOLS);
    let info = lead.peer_info().unwrap();
    assert_eq!(info.instructions.as_deref(), Some(riff::text::MCP_LEAD));
    let start = riff::text::lead_start("como-technologies/riff");
    assert!(start.contains("whoami, notes and read"), "{start}");

    // A lead with no notes.
    let (none, is_error) = call(&lead, "notes", serde_json::json!({})).await;
    assert!(!is_error && none.contains("has no notes"), "{none}");

    let write = |text: &str| serde_json::json!({ "text": text });
    let (saved, is_error) =
        call(&lead, "notes", write("Wave 36 runs. Mike decides the cut.")).await;
    assert!(!is_error && saved.contains("are saved"), "{saved}");
    let (same, _) = call(&lead, "notes", write("Wave 36 runs. Mike decides the cut.")).await;
    assert!(same.contains("have this text already"), "{same}");
    call(&lead, "notes", write("Wave 37 runs.")).await;

    let (now, is_error) = call(&lead, "notes", serde_json::json!({})).await;
    assert!(!is_error, "{now}");
    assert!(now.contains("Wave 37 runs."), "{now}");
    assert!(!now.contains("Wave 36"), "{now}");
    let (all, _) = call(&lead, "notes", serde_json::json!({ "history": true })).await;
    assert!(
        all.contains("Wave 37 runs.") && all.contains("Before:"),
        "{all}"
    );
    assert!(all.contains("Wave 36 runs. Mike decides the cut."), "{all}");

    // The refusals.
    let (secret, is_error) = call(
        &lead,
        "notes",
        write("key ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
    )
    .await;
    assert!(
        is_error && secret.contains("looks like it has a secret"),
        "{secret}"
    );
    assert!(!secret.contains("ghp_"), "{secret}");
    let (long, is_error) = call(&lead, "notes", write(&"a".repeat(64 * 1024 + 1))).await;
    assert!(is_error && long.contains("64 KiB"), "{long}");
    let (still, _) = call(&lead, "notes", serde_json::json!({})).await;
    assert!(still.contains("Wave 37 runs."), "{still}");

    // Another user does not read the notes of mike. A worker cannot
    // write them.
    let brett: SessionUri = BRETT.parse().unwrap();
    of(&api, &brett).register(&brett).await.unwrap();
    let thread = brett.default_thread().unwrap();
    let error = of(&api, &brett)
        .notes_show(&brett, &thread, Some("mike"), false)
        .await;
    let error = format!("{:#}", error.unwrap_err());
    assert!(error.contains("only mike and the owner"), "{error}");
    let worker: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=w1#w1"
        .parse()
        .unwrap();
    of(&api, &worker).register_as(&worker, true).await.unwrap();
    let error = of(&api, &worker).notes_set(&worker, &thread, "x").await;
    let error = format!("{:#}", error.unwrap_err());
    assert!(
        error.contains("a worker cannot write the notes of the lead"),
        "{error}"
    );
}

/// Riff 4.0 (#1173) removed `riff mcp`: the tools of a session in a
/// container, which `riff relay mcp` serves, and the tools of the lead
/// are the same as before (01M59BXNPR3AZ0HAS9GQCZ4PH2).
#[tokio::test]
async fn the_tools_of_a_container_session_and_of_the_lead_are_as_before() {
    let api = start_server().await;
    for (me, lead, want) in [
        (
            MIKE,
            false,
            &[
                "blocked", "post", "read", "status", "tell", "threads", "who", "whoami",
            ][..],
        ),
        (
            BRETT,
            true,
            &[
                "blocked", "free", "hold", "notes", "pause", "post", "read", "resume", "status",
                "tell", "threads", "who", "whoami",
            ][..],
        ),
    ] {
        let me: SessionUri = me.parse().unwrap();
        of(&api, &me).register(&me).await.unwrap();
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let tools = Tools::new(of(&api, &me), me);
        let tools = if lead {
            tools.in_lead_container()
        } else {
            tools.in_container()
        };
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
        let mut listed: Vec<_> = client
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        listed.sort();
        assert_eq!(listed, want);
    }
}
