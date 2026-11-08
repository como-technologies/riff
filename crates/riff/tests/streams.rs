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
use riff_client::api::follow;
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

async fn start_server() -> (crate::common::Signed, Seen) {
    let seen = Seen::default();
    let layer = seen.clone();
    let server = crate::common::start_signed_with(move |router| {
        router.layer(axum::middleware::from_fn_with_state(layer, record))
    })
    .await;
    (server, seen)
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
    let (lead_api, person_api) = (server.api("mike", Some("l1")), server.api("mike", None));
    let lead: SessionUri = "riff://mike@a/local/riff?session=l1".parse().unwrap();
    lead_api.register(&lead).await.unwrap();
    crate::common::name_lead(&person_api, &lead).await;
    lead_api.set_riff(&lead, RiffState::Running).await.unwrap();

    // The host: a client of its own, with a status call first, as `riff
    // workers host` makes it. The calls repeat until the pool gives a
    // call the connection of the call before it.
    let api = server.api("mike", Some("h1"));
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
    lead_api.tell(&lead, "h1", "hello").await.unwrap();
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

/// A new watch prints no line for a message that a `read` returned: a
/// plain `read`, and a `read` of `all` after a number. A new addressed
/// message still wakes the watch.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_watch_has_no_wake_for_a_message_that_a_read_returned() {
    use riff_core::name::ThreadName;
    use riff_core::selector::Selector;
    use riff_core::wire::Kind;

    for skip in [false, true] {
        let (server, _seen) = start_server().await;
        let lead: SessionUri = "riff://mike@a/local/riff?session=l1".parse().unwrap();
        let (lead_api, person_api) = (server.api("mike", Some("l1")), server.api("mike", None));
        lead_api.register(&lead).await.unwrap();
        crate::common::name_lead(&person_api, &lead).await;
        lead_api.set_riff(&lead, RiffState::Running).await.unwrap();
        let me: SessionUri = "riff://mike@b/local/riff?session=h1".parse().unwrap();
        let me_api = server.api("mike", Some("h1"));
        me_api.register(&me).await.unwrap();

        let thread: ThreadName = "chat".parse().unwrap();
        let to = [Selector::session("h1")];
        for body in ["one", "two"] {
            lead_api
                .post(&lead, Some(&thread), &to, body, Kind::Message)
                .await
                .unwrap();
        }
        // The read shows the messages. With `skip`, it reads all after 1
        // (01M6WBQQEGZKVV49QWCQGTE4CY).
        let (shown, _) = me_api
            .read_page(&me, &thread, skip, skip.then_some(1))
            .await
            .unwrap();
        assert!(!shown.is_empty());

        // The watch is `riff relay watch --follow`: it prints a line for
        // each wake.
        let mut wakes = Box::pin(follow(|| me_api.watch(&me), Duration::from_secs(5)));
        let none = isolated::in_time(Duration::from_secs(2), wakes.next()).await;
        assert!(none.is_err(), "skip {skip}: a wake for a read message");
        lead_api
            .post(&lead, Some(&thread), &to, "three", Kind::Message)
            .await
            .unwrap();
        let wake = isolated::in_time(Duration::from_secs(30), wakes.next()).await;
        assert!(matches!(wake, Ok(Some(Ok(_)))), "skip {skip}: no wake");
    }
}

/// A read of `all` after a number marks the message of a wake read, also
/// with an unread backlog before the number (01M6WBQQEGZKVV49QWCQGTE4CY,
/// #1690). The checkpoint keeps the read cursor, so a new watch after a
/// start of the server prints no line for that message. The next message
/// to the session still wakes the watch.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_watch_after_a_start_has_no_wake_for_a_message_that_a_read_of_all_returned() {
    use riff_core::name::ThreadName;
    use riff_core::selector::Selector;
    use riff_core::wire::Kind;
    use riff_state::store::{Memory, Store};

    let store: Arc<dyn Store> = Arc::new(Memory::default());
    let lease = riff_server::lease::Timing {
        wait: Duration::from_millis(50),
        read_every: Duration::from_millis(50),
        valid_for: Duration::from_millis(500),
        exit_after: Duration::from_secs(1),
        ..Default::default()
    };
    let start = |store: Arc<dyn Store>| async {
        let (listener, url) = crate::common::listen().await;
        let loaded = Some((store, lease));
        crate::common::start_signed_at(listener, &url, loaded, |router| router).await
    };
    let lead: SessionUri = "riff://mike@a/local/riff?session=l1".parse().unwrap();
    let me: SessionUri = "riff://mike@b/local/riff?session=h1".parse().unwrap();
    let thread: ThreadName = "chat".parse().unwrap();
    let to = [Selector::session("h1")];

    let old = start(store.clone()).await;
    let (lead_api, person_api) = (old.api("mike", Some("l1")), old.api("mike", None));
    lead_api.register(&lead).await.unwrap();
    crate::common::name_lead(&person_api, &lead).await;
    lead_api.set_riff(&lead, RiffState::Running).await.unwrap();
    let me_api = old.api("mike", Some("h1"));
    me_api.register(&me).await.unwrap();
    // The session joins first, so each later message is unread: the
    // backlog of a lead.
    me_api.join(&me, &thread).await.unwrap();
    lead_api
        .post(&lead, Some(&thread), &[], "old", Kind::Message)
        .await
        .unwrap();
    let n = lead_api
        .post(&lead, Some(&thread), &to, "for you", Kind::Message)
        .await
        .unwrap()
        .seq;
    let (shown, _) = me_api
        .read_page(&me, &thread, true, Some(n - 1))
        .await
        .unwrap();
    let seqs: Vec<u64> = shown.iter().map(|c| c.message.seq).collect();
    assert_eq!(seqs, [n]);
    old.service.shutdown().await.unwrap();

    let new = start(store).await;
    let (lead_api, me_api) = (new.api("mike", Some("l1")), new.api("mike", Some("h1")));
    // A session is gone after a start until it calls. The riff daemon
    // sends its keep-alives.
    me_api.alive(&me).await.unwrap();
    let mut wakes = Box::pin(follow(|| me_api.watch(&me), Duration::from_secs(5)));
    let none = isolated::in_time(Duration::from_secs(2), wakes.next()).await;
    assert!(none.is_err(), "a wake for message {n} after a start");
    let posted = lead_api
        .post(&lead, Some(&thread), &to, "next", Kind::Message)
        .await
        .unwrap();
    assert_eq!(posted.woken.len(), 1, "{posted:?}");
    match isolated::in_time(Duration::from_secs(30), wakes.next()).await {
        Ok(Some(Ok(wake))) => assert_eq!(wake.seq, n + 1),
        other => panic!("no wake for message {}: {other:?}", n + 1),
    }
}
