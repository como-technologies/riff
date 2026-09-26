//! The slice 1 loop over real HTTP: a mention and a direct message wake
//! a watching session, `tail` shows posts, and claims block.

use std::time::Duration;

use futures::StreamExt;
use riff::api::Api;
use riff_core::name::{SessionName, ThreadName};
use riff_core::wire::WakeReason;

const WAIT: Duration = Duration::from_secs(5);

async fn start_server() -> Api {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, riff_server::router()).await.unwrap();
    });
    Api::new(&format!("http://{addr}"))
}

fn name(text: &str) -> SessionName {
    text.parse().unwrap()
}

#[tokio::test]
async fn mentions_and_direct_messages_wake_a_watching_session() {
    let api = start_server().await;
    let mike = name("riff://mike@pangolin/como-technologies/riff#api");
    let brett = name("riff://brett@heron/como-technologies/riff#tests");
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();
    api.register(&mike).await.unwrap();

    let mut wakes = Box::pin(api.watch(&brett).await.unwrap());
    let mut tail = Box::pin(api.tail(&thread).await.unwrap());

    let who = api.who().await.unwrap();
    assert!(who.iter().any(|s| s.name == brett && s.live));
    assert!(who.iter().any(|s| s.name == mike && !s.live));

    api.post(&mike, &thread, "no mention here").await.unwrap();
    api.post(&mike, &thread, "@brett@heron:riff#tests the API is ready")
        .await
        .unwrap();
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.reason, WakeReason::Mention);
    assert_eq!(wake.seq, 2, "the post without a mention must not wake");
    assert_eq!(wake.from, mike);

    let first = tokio::time::timeout(WAIT, tail.next()).await.unwrap().unwrap().unwrap();
    assert_eq!(first.message.body, "no mention here");

    api.tell(&mike, &brett, "a direct message").await.unwrap();
    let wake = tokio::time::timeout(WAIT, wakes.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(wake.reason, WakeReason::Direct);

    let unread = api.read(&brett, &wake.thread, false).await.unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].body, "a direct message");
}

#[tokio::test]
async fn a_claim_blocks_a_second_session() {
    let api = start_server().await;
    let mike = name("riff://mike@pangolin/como-technologies/riff#api");
    let brett = name("riff://brett@heron/como-technologies/riff#tests");
    let thread: ThreadName = "como-technologies/riff".parse().unwrap();

    assert!(api.claim(&mike, &thread, "issue-12").await.unwrap().granted);
    let reply = api.claim(&brett, &thread, "issue-12").await.unwrap();
    assert!(!reply.granted);
    assert_eq!(reply.holder, mike);
    assert!(api.release(&brett, &thread, "issue-12").await.is_err());
    api.release(&mike, &thread, "issue-12").await.unwrap();
    assert!(api.claim(&brett, &thread, "issue-12").await.unwrap().granted);
}
