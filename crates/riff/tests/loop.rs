//! The loop over real HTTP: an address and a direct message wake a
//! watching session, `tail` shows posts, claims block, and a moved
//! session keeps its watch.

use std::time::Duration;

use futures::StreamExt;
use riff::api::Api;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::selector::Selector;

const WAIT: Duration = Duration::from_secs(5);

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn uri(text: &str) -> SessionUri {
    text.parse().unwrap()
}

fn to(text: &str) -> Vec<Selector> {
    vec![text.parse().unwrap()]
}

fn mike() -> SessionUri {
    uri("riff://mike@pangolin/como-technologies/riff?session=a1#api")
}

fn brett() -> SessionUri {
    uri("riff://brett@heron/como-technologies/riff?session=b2#tests")
}

fn repo() -> ThreadName {
    "como-technologies/riff".parse().unwrap()
}

#[tokio::test]
async fn addresses_and_direct_messages_wake_a_watching_session() {
    let api = start_server().await;
    let (mike, brett, thread) = (mike(), brett(), repo());
    api.register(&mike).await.unwrap();

    let mut wakes = Box::pin(api.watch(&brett).await.unwrap());
    let mut tail = Box::pin(api.tail(&thread).await.unwrap());

    let who = api.who(&mike, false).await.unwrap();
    assert!(who.iter().any(|s| s.uri == brett && s.live));
    assert!(who.iter().any(|s| s.uri == mike && !s.live));
    // A call of who counts as a call: a new session is listed.
    let docs = uri("riff://mike@pangolin/como-technologies/riff?session=c3#docs");
    let who = api.who(&docs, false).await.unwrap();
    assert!(who.iter().any(|s| s.uri == docs && s.idle_secs == 0));

    api.post(&mike, Some(&thread), &[], "@brett in text does not wake")
        .await
        .unwrap();
    let posted = api
        .post(&mike, Some(&thread), &to("user=brett"), "the API is ready")
        .await
        .unwrap();
    assert_eq!(posted.woken, vec![brett.clone()]);
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.seq, 2, "the post without an address must not wake");
    assert_eq!(wake.from, mike);

    let first = tokio::time::timeout(WAIT, tail.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first.message.body, "@brett in text does not wake");

    api.post(&mike, None, &to("session=b2"), "a direct message")
        .await
        .unwrap();
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(wake.thread.is_direct());

    let unread = api.read(&brett, &wake.thread, false).await.unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].body, "a direct message");
}

#[tokio::test]
async fn a_moved_session_keeps_its_watch() {
    let api = start_server().await;
    let (mike, brett, thread) = (mike(), brett(), repo());
    api.register(&brett).await.unwrap();
    let mut wakes = Box::pin(api.watch(&brett).await.unwrap());

    let moved = uri("riff://brett@heron/como-technologies/riff?session=b2#issue-6");
    api.register(&moved).await.unwrap();
    let who = api.who(&moved, false).await.unwrap();
    assert_eq!(who.len(), 1, "a move must not make a second session");

    api.post(&mike, Some(&thread), &to("worktree=issue-6"), "hi")
        .await
        .unwrap();
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.from.who(), mike.who());
}

#[tokio::test]
async fn a_claim_blocks_a_second_session_and_is_addressable() {
    let api = start_server().await;
    let (mike, brett, thread) = (mike(), brett(), repo());

    assert!(api.claim(&mike, &thread, "issue-12").await.unwrap().granted);
    let reply = api.claim(&brett, &thread, "issue-12").await.unwrap();
    assert!(!reply.granted);
    assert_eq!(reply.holder.who(), mike.who());
    assert_eq!(reply.holder.claims(), ["issue-12"]);

    let posted = api
        .post(&brett, Some(&thread), &to("claim=issue-12"), "status?")
        .await
        .unwrap();
    assert_eq!(posted.woken[0].who(), mike.who());

    assert!(api.release(&brett, &thread, "issue-12").await.is_err());
    api.release(&mike, &thread, "issue-12").await.unwrap();
    assert!(
        api.claim(&brett, &thread, "issue-12")
            .await
            .unwrap()
            .granted
    );
}

#[tokio::test]
async fn a_watch_starts_with_a_wake_for_a_missed_message() {
    let api = start_server().await;
    let (mike, brett, thread) = (mike(), brett(), repo());
    api.register(&brett).await.unwrap();
    api.post(
        &mike,
        Some(&thread),
        &to("user=brett"),
        "while you were away",
    )
    .await
    .unwrap();

    let mut wakes = Box::pin(api.watch(&brett).await.unwrap());
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.seq, 1);

    // After a read, a new watch has nothing to report.
    api.read(&brett, &thread, false).await.unwrap();
    drop(wakes);
    let mut wakes = Box::pin(api.watch(&brett).await.unwrap());
    assert!(
        tokio::time::timeout(Duration::from_millis(300), wakes.next())
            .await
            .is_err()
    );
}
