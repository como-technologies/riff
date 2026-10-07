//! The rule for a new try of the link (01M4A8041F8EK1VYDE4C9QG8N8), the
//! call ID of each call (01M4A8048J60YSVNVYF2432KE8), the shared link of
//! a server (01M4A803WN0KTDGGAX2E771XDF, 01M4A8043S2ZCKRH19Z3Q8AJ1F) and
//! the budget of a tool call (01M4A803Z4Q0KX6NT1KC6QR43H). The raw server
//! of a test gives the replies of its script, one for each request, and
//! keeps the call ID of each request. The limits are in milliseconds.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use riff::api::Api;
use riff::link::{Limits, SHORT_BUDGET};
use riff::mcp::Tools;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{CALL_HEADER, Call, Kind};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The longest time that a test waits for a fact.
const WAIT: Duration = Duration::from_secs(20);

/// What the raw server does with one request.
#[derive(Clone, Copy, Debug)]
enum Reply {
    /// The reply of `riff-server` to `register`: 200 with its build.
    Good,
    /// A head and the start of the body, then the end of the connection.
    Cut,
    /// No reply: it keeps the connection open.
    Never,
    /// A reply with this status, with the build header or not.
    Status(u16, bool),
}

/// The requests that the raw server got: the call ID of each.
#[derive(Default)]
struct Seen {
    ids: Mutex<Vec<Option<String>>>,
    accepts: AtomicUsize,
}

/// Starts a raw server that gives `script`, one reply for each request,
/// and [`Reply::Good`] after the end of the script.
async fn raw_server(script: Vec<Reply>) -> (String, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Seen::default());
    let script = Arc::new(script);
    let (count, server_seen) = (Arc::new(AtomicUsize::new(0)), seen.clone());
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            server_seen.accepts.fetch_add(1, Ordering::SeqCst);
            let (script, count, seen) = (script.clone(), count.clone(), server_seen.clone());
            tokio::spawn(serve(socket, script, count, seen));
        }
    });
    (url, seen)
}

/// Serves the requests of one connection.
async fn serve(
    mut socket: TcpStream,
    script: Arc<Vec<Reply>>,
    count: Arc<AtomicUsize>,
    seen: Arc<Seen>,
) {
    let mut buffer = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let head_end = loop {
            if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break end + 4;
            }
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            }
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).to_lowercase();
        let header = |name: &str| {
            head.lines()
                .find_map(|l| l.strip_prefix(&format!("{name}: ")))
                .map(|v| v.trim().to_owned())
        };
        let length: usize = header("content-length").map_or(0, |v| v.parse().unwrap());
        while buffer.len() < head_end + length {
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            }
        }
        buffer.drain(..head_end + length);
        seen.ids.lock().unwrap().push(header(CALL_HEADER));
        let n = count.fetch_add(1, Ordering::SeqCst);
        let build = format!(
            "{}: {}\r\n",
            riff_core::build::HEADER,
            riff_core::build::VERSION
        );
        let reply = |status: &str, build: &str, body: &str, length: usize| {
            format!("HTTP/1.1 {status}\r\n{build}content-length: {length}\r\n\r\n{body}")
        };
        let bytes = match script.get(n).copied().unwrap_or(Reply::Good) {
            Reply::Good => reply("200 OK", &build, "null", 4),
            Reply::Cut => reply("200 OK", &build, "nu", 4),
            Reply::Never => {
                let _ = socket.read(&mut chunk).await;
                return std::future::pending().await;
            }
            Reply::Status(code, with_build) => {
                let build = if with_build { build.as_str() } else { "" };
                reply(&format!("{code} Busy"), build, "busy", 4)
            }
        };
        if socket.write_all(bytes.as_bytes()).await.is_err() {
            return;
        }
        if matches!(script.get(n), Some(Reply::Cut)) {
            return;
        }
    }
}

fn me() -> SessionUri {
    "riff://mike@pangolin/como-technologies/riff?session=a6cf"
        .parse()
        .unwrap()
}

/// Limits in milliseconds: a try of 300 ms, a budget of 1.5 s.
fn fast() -> Limits {
    Limits {
        connect: Duration::from_millis(500),
        try_wait: Duration::from_millis(300),
        stream_idle: Duration::from_millis(300),
        budget: Duration::from_millis(1500),
        first_wait: Duration::from_millis(20),
        most_wait: Duration::from_millis(100),
    }
}

/// Each fault of the rule gets a new try with the same call ID, and the
/// call gets the reply of the next try: a cut, no reply in time, a 502,
/// a 503 with and with no build header, a 429.
#[tokio::test]
async fn each_fault_gets_a_new_try_with_the_same_call_id() {
    let faults = [
        Reply::Cut,
        Reply::Never,
        Reply::Status(502, false),
        Reply::Status(503, false),
        Reply::Status(503, true),
        Reply::Status(429, false),
    ];
    for fault in faults {
        let (url, seen) = raw_server(vec![fault]).await;
        let api = Api::new(&url).with_limits(fast());
        let call = isolated::in_time(WAIT, api.register(&me())).await;
        let call = call.expect("the call ends in time");
        call.unwrap_or_else(|e| panic!("{fault:?}: {e:#}"));
        let ids = seen.ids.lock().unwrap().clone();
        assert_eq!(ids.len(), 2, "{fault:?}: {ids:?}");
        assert!(ids[0].is_some(), "{fault:?}: no call ID");
        assert_eq!(ids[0], ids[1], "{fault:?}: two call IDs");
    }
}

/// A refusal of `riff-server` gets no new try.
#[tokio::test]
async fn a_refusal_gets_no_new_try() {
    let (url, seen) = raw_server(vec![Reply::Status(409, true)]).await;
    let api = Api::new(&url).with_limits(fast());
    let error = api.register(&me()).await.unwrap_err();
    assert!(error.to_string().contains("409"), "{error:#}");
    assert_eq!(seen.ids.lock().unwrap().len(), 1);
}

/// Two calls have two call IDs.
#[tokio::test]
async fn each_call_has_a_call_id_of_its_own() {
    let (url, seen) = raw_server(vec![]).await;
    let api = Api::new(&url).with_limits(fast());
    api.register(&me()).await.unwrap();
    api.register(&me()).await.unwrap();
    let ids = seen.ids.lock().unwrap().clone();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

/// A call with no reply in its whole budget ends after the budget, with
/// the text that names the budget. Each try ends after the limit of a
/// try, and the next try comes on a new connection.
#[tokio::test]
async fn a_call_with_no_reply_ends_after_its_budget() {
    let (url, seen) = raw_server(vec![Reply::Never; 100]).await;
    let api = Api::new(&url).with_limits(fast());
    let span = isolated::Span::start();
    let call = isolated::in_time(WAIT, api.register(&me())).await;
    let error = call.expect("the call ends in time").unwrap_err();
    assert!(
        span.wall() >= Duration::from_millis(600),
        "{:?}",
        span.wall()
    );
    assert!(span.within(Duration::from_secs(10)), "{:?}", span.wall());
    assert_eq!(error.to_string(), riff::text::no_reply(&url, fast().budget));
    let tries = seen.ids.lock().unwrap().len();
    assert!(tries >= 2, "{tries} tries");
    assert_eq!(seen.accepts.load(Ordering::SeqCst), tries);
}

/// The status line makes one try: a fault ends its call.
#[tokio::test]
async fn one_try_makes_one_try() {
    let (url, seen) = raw_server(vec![Reply::Status(503, false)]).await;
    let api = Api::new(&url)
        .with_limits(fast())
        .one_try(Duration::from_millis(300));
    assert!(api.register(&me()).await.is_err());
    assert_eq!(seen.ids.lock().unwrap().len(), 1);
}

/// Two `Api` of one server share one link. A fault with no reply makes
/// one new client of the calls, for both.
#[tokio::test]
async fn two_api_of_one_server_share_one_link() {
    let (url, _seen) = raw_server(vec![Reply::Cut]).await;
    let one = Api::new(&url).with_limits(fast());
    let two = Api::new(&url).with_limits(fast());
    assert!(Arc::ptr_eq(one.link(), two.link()));
    assert_eq!(two.link().swaps(), 0);
    one.register(&me()).await.unwrap();
    assert_eq!(one.link().swaps(), 1);
    assert_eq!(two.link().swaps(), 1);
    // A 503 came on a good connection: it keeps the client.
    let (url, _seen) = raw_server(vec![Reply::Status(503, true)]).await;
    let api = Api::new(&url).with_limits(fast());
    api.register(&me()).await.unwrap();
    assert_eq!(api.link().swaps(), 0);
}

/// A `riff-server` whose first reply to a post is cut, after it ran the
/// post.
async fn cut_first_post() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cuts = Arc::new(AtomicUsize::new(0));
    let posts = cuts.clone();
    let router = riff_server::router().layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let cuts = cuts.clone();
            async move {
                let post = request.uri().path() == riff_core::wire::Post::PATH;
                let response = next.run(request).await;
                if !post || cuts.fetch_add(1, Ordering::SeqCst) > 0 {
                    return response;
                }
                // The server ran the post: the reply breaks in its body.
                let (parts, _) = response.into_parts();
                let broken = futures::stream::iter([Err::<axum::body::Bytes, _>(
                    std::io::Error::other("cut"),
                )]);
                axum::response::Response::from_parts(parts, axum::body::Body::from_stream(broken))
            }
        },
    ));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, posts)
}

/// A post whose first reply is cut makes one message, and the command
/// gets the reply of its new try (01M48VFX22S4811DYBBD7QDW24).
#[tokio::test]
async fn a_post_whose_reply_is_cut_makes_one_message() {
    let (url, posts) = cut_first_post().await;
    let api = Api::new(&url).with_limits(fast());
    let me = me();
    api.register(&me).await.unwrap();
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    let posted = api.post(&me, Some(&thread), &[], "hello", Kind::Note).await;
    posted.expect("the post gets the reply of its new try");
    let read = api.read(&me, &thread, true).await.unwrap();
    let hellos = read.iter().filter(|c| c.message.body == "hello").count();
    assert_eq!(hellos, 1, "one message");
    assert_eq!(posts.load(Ordering::SeqCst), 2, "the post came two times");
}

/// A tool call of `riff mcp` with the server away ends after its budget
/// with the text of the fault, and the next tool call runs.
#[tokio::test]
async fn a_tool_call_with_the_server_away_ends_after_its_budget() {
    let (url, _seen) = raw_server(vec![Reply::Never; 100]).await;
    let api = Api::new(&url).with_limits(fast());
    let client = connect(Tools::new(api, me())).await;
    for _ in 0..2 {
        let span = isolated::Span::start();
        let (text, error) = isolated::in_time(WAIT, call(&client, "who")).await.unwrap();
        assert!(error, "{text}");
        assert!(
            text.contains(&riff::text::no_reply(&url, fast().budget)),
            "{text}"
        );
        assert!(span.within(Duration::from_secs(10)), "{:?}", span.wall());
    }
}

/// The budget of a tool call is the budget of a short command.
#[test]
fn a_tool_call_has_the_budget_of_a_short_command() {
    assert_eq!(Limits::default().budget, SHORT_BUDGET);
    assert_eq!(SHORT_BUDGET, Duration::from_secs(60));
}

/// Connects an MCP client to `tools`.
async fn connect(tools: Tools) -> RunningService<RoleClient, ()> {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let running = tools.serve(server_io).await.unwrap();
        let _ = running.waiting().await;
    });
    ().serve(client_io).await.unwrap()
}

/// Calls the tool `tool` with no arguments: its text, and true for an
/// error.
async fn call(client: &RunningService<RoleClient, ()>, tool: &str) -> (String, bool) {
    let params = CallToolRequestParams::new(tool.to_owned());
    let result = client.call_tool(params).await.unwrap();
    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    (text, result.is_error == Some(true))
}
