//! One rule for who reads a thread, on `read`, `tail` and `watch`: the
//! caller acts as its token, and gets a direct thread only when it is
//! one of its two sessions.

mod common;

use std::time::Duration;

use futures::StreamExt;
use riff_core::name::{SessionUri, ThreadName};
use serde_json::{Value, json};
use tokio::time::timeout;

const ANN: &str = "riff://ann@heron/acme/app?session=a";
const BOB: &str = "riff://bob@heron/acme/app?session=b";
const CY: &str = "riff://cy@heron/acme/app?session=c";

async fn call(base: &str, op: &str, body: Value) -> reqwest::Response {
    common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// A riff with ann, bob and cy. Gives the base URL and the direct thread
/// of ann and bob.
async fn riff() -> (riff_server::Service, String, ThreadName) {
    let (service, base) = common::start(false, &[]).await;
    for me in [ANN, BOB, CY] {
        assert_eq!(
            call(&base, "register", json!({ "me": me })).await.status(),
            200
        );
    }
    let (ann, bob): (SessionUri, SessionUri) = (ANN.parse().unwrap(), BOB.parse().unwrap());
    (service, base, ThreadName::direct(ann.who(), bob.who()))
}

async fn direct_message(base: &str) {
    let post = json!({ "me": ANN, "to": [{ "session": "b" }], "body": "secret" });
    assert_eq!(call(base, "post", post).await.status(), 200);
}

/// Each data line of an event stream, until `wait` passes.
async fn events(response: reqwest::Response, wait: Duration) -> Vec<String> {
    let mut lines = Vec::new();
    let mut stream = response.bytes_stream();
    let _ = timeout(wait, async {
        while let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8_lossy(&chunk).into_owned();
            lines.extend(
                text.lines()
                    .filter(|l| l.starts_with("data:"))
                    .map(str::to_owned),
            );
        }
    })
    .await;
    lines
}

#[tokio::test]
async fn read_gives_a_direct_thread_only_to_its_two_sessions() {
    let (_service, base, direct) = riff().await;
    direct_message(&base).await;
    let read = |me: &str| json!({ "me": me, "thread": direct.to_string(), "all": true });
    let reply: Value = call(&base, "read", read(BOB)).await.json().await.unwrap();
    assert_eq!(reply["messages"][0]["body"], "secret");
    let reply: Value = call(&base, "read", read(ANN)).await.json().await.unwrap();
    assert_eq!(reply["messages"][0]["body"], "secret");
    assert_eq!(call(&base, "read", read(CY)).await.status(), 404);
}

#[tokio::test]
async fn tail_gives_a_direct_thread_only_to_its_two_sessions() {
    let (_service, base, direct) = riff().await;
    let tail = |me: &str| {
        common::client()
            .get(format!("{base}/v1/tail"))
            .query(&[("uri", me), ("thread", &direct.to_string())])
            .send()
    };
    assert_eq!(tail(CY).await.unwrap().status(), 404);
    let bob = tail(BOB).await.unwrap();
    assert_eq!(bob.status(), 200);
    direct_message(&base).await;
    let lines = events(bob, Duration::from_millis(500)).await;
    assert!(lines.iter().any(|l| l.contains("secret")), "{lines:?}");
}

#[tokio::test]
async fn watch_gives_no_wake_of_a_direct_thread_of_others() {
    let (_service, base, _) = riff().await;
    let watch = |me: &str| {
        common::client()
            .get(format!("{base}/v1/watch"))
            .query(&[("uri", me)])
            .send()
    };
    let (bob, cy) = (watch(BOB).await.unwrap(), watch(CY).await.unwrap());
    direct_message(&base).await;
    let (bob, cy) = tokio::join!(
        events(bob, Duration::from_millis(500)),
        events(cy, Duration::from_millis(500))
    );
    assert_eq!(bob.len(), 1, "{bob:?}");
    assert!(cy.is_empty(), "{cy:?}");
}
