//! A stream has a connection of its own (01M3WN72ECF0WKR4M7M6ZYAF9J).
//! The server of the test records the path and the client port of each
//! request. A client port names one connection.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use futures::StreamExt;
use riff::api::{Api, follow};
use riff_core::name::SessionUri;
use riff_core::wire::{RiffState, Status};

/// The path and the client port of each request, in order.
type Seen = Arc<Mutex<Vec<(String, u16)>>>;

async fn record(
    State(seen): State<Seen>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    seen.lock().unwrap().push((path, peer.port()));
    next.run(request).await
}

async fn start_server() -> (Api, Seen) {
    let seen = Seen::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = Api::new(&format!("http://{}", listener.local_addr().unwrap()));
    let router =
        riff_server::router().layer(axum::middleware::from_fn_with_state(seen.clone(), record));
    tokio::spawn(async move {
        let service = router.into_make_service_with_connect_info::<SocketAddr>();
        axum::serve(listener, service).await.unwrap();
    });
    (api, seen)
}

/// The client ports of the requests to `path`.
fn ports(seen: &Seen, path: &str) -> Vec<u16> {
    let seen = seen.lock().unwrap();
    seen.iter()
        .filter(|(p, _)| p == path)
        .map(|(_, port)| *port)
        .collect()
}

/// A watch opens a connection of its own, also when the pool holds an
/// idle connection of a call. No call uses the connection of the open
/// watch, and each call gets its reply. Before, the watch took the idle
/// connection of the status call, the pool gave that connection to the
/// next call, and that call got no reply while the watch was open.
#[tokio::test(flavor = "multi_thread")]
async fn a_watch_has_a_connection_of_its_own() {
    let (server, seen) = start_server().await;
    let lead: SessionUri = "riff://mike@a/local/riff?session=l1".parse().unwrap();
    server.register(&lead).await.unwrap();
    server.set_riff(&lead, RiffState::Running).await.unwrap();

    // The host: a client of its own, with a status call first, as `riff
    // workers host` makes it. The calls repeat until the pool gives a
    // call the connection of the call before it.
    let api = Api::new(server.base());
    let me: SessionUri = "riff://mike@b/local/riff?session=h1".parse().unwrap();
    let status = Status {
        step: "workers host: limit 1, floor 4GB, no workers".into(),
    };
    loop {
        api.status(&me, &status).await.unwrap();
        // The pool takes the connection back just after the reply.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let calls = ports(&seen, "/v1/status");
        if calls.len() >= 2 && calls[calls.len() - 1] == calls[calls.len() - 2] {
            break;
        }
        assert!(calls.len() < 50, "the pool gave no connection again");
    }
    let calls = ports(&seen, "/v1/status");

    let mut wakes = Box::pin(follow(|| api.watch(&me), Duration::from_secs(5)));
    server.tell(&lead, "h1", "hello").await.unwrap();
    let wake = isolated::in_time(Duration::from_secs(30), wakes.next()).await;
    assert!(matches!(wake, Ok(Some(Ok(_)))), "no wake came");
    let watch = ports(&seen, "/v1/watch");
    assert_eq!(watch.len(), 1, "{watch:?}");
    assert!(
        !calls.contains(&watch[0]),
        "the watch took the connection of a call: {watch:?} {calls:?}"
    );

    // Each call gets its reply while the watch is open.
    for _ in 0..5 {
        let inbox = isolated::in_time(Duration::from_secs(30), api.inbox(&me, None, true));
        assert!(inbox.await.is_ok(), "a call got no reply");
    }
    let seen = seen.lock().unwrap();
    let on_the_watch: Vec<_> = seen
        .iter()
        .filter(|(path, port)| *port == watch[0] && path != "/v1/watch")
        .collect();
    assert!(on_the_watch.is_empty(), "{on_the_watch:?}");
    drop(wakes);
}
