//! `GET /v1/server` gives the facts of the server, for `riff server`
//! (01M3TJWJ12WEDCXW3W0529KRP2). It answers also while the server replies
//! 503 to each other call.

use crate::common;

use std::sync::Arc;

use riff_core::wire::ServerFacts;
use riff_server::store::Memory;
use serde_json::{Value, json};

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";

async fn get(base: &str) -> reqwest::Response {
    common::client()
        .get(format!("{base}/v1/server"))
        .send()
        .await
        .unwrap()
}

async fn post(base: &str, op: &str, body: Value) -> reqwest::Response {
    common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn the_reply_names_each_fact() {
    let (_service, base) = common::start_on(Arc::new(Memory::default())).await;
    assert_eq!(
        post(&base, "register", json!({ "me": A })).await.status(),
        200
    );

    let reply = get(&base).await;
    assert_eq!(reply.status(), 200);
    assert!(reply.headers().contains_key(riff_core::build::HEADER));
    let json: Value = reply.json().await.unwrap();
    // Each fact of "Monitoring" has its field.
    for field in [
        "not_serving",
        "last_error",
        "position",
        "chunk_written_at_ms",
        "chunk_write_ms",
        "write_errors",
        "skipped_records",
        "checkpoint",
        "no_checkpoint",
        "chunks",
        "sessions",
        "cursors",
        "threads",
        "sign_ins",
        "memory_bytes",
        "started_at_ms",
        "replay_ms",
        "now_ms",
    ] {
        assert!(json.get(field).is_some(), "no {field} in {json}");
    }
    let facts: ServerFacts = serde_json::from_value(json).unwrap();
    assert_eq!(facts.not_serving, None);
    assert!(facts.position >= 1, "the register made a record");
    assert_eq!((facts.sessions, facts.threads), (1, 1));
    assert!(facts.chunks >= 1 && facts.chunk_written_at_ms.is_some());
}

#[tokio::test]
async fn the_facts_answer_while_each_other_call_gets_503() {
    let (service, base) = common::start_on(Arc::new(Memory::default())).await;
    service.shutdown().await.unwrap();

    let who = post(&base, "who", json!({ "me": A })).await;
    assert_eq!(who.status(), 503);
    let reply = get(&base).await;
    assert_eq!(reply.status(), 200);
    let facts: ServerFacts = reply.json().await.unwrap();
    assert_eq!(facts.not_serving.as_deref(), Some("it shuts down"));
}

#[tokio::test]
async fn the_facts_answer_a_riff_of_each_version() {
    let (_service, base) = common::start(false, &[]).await;
    let reply = reqwest::Client::new()
        .get(format!("{base}/v1/server"))
        .header(riff_core::build::HEADER, "0.1.0 abc 2026-01-01T00:00:00Z")
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
}

#[tokio::test]
async fn a_riff_with_sign_in_gives_the_facts_only_to_a_caller_with_a_token() {
    let (_service, base) = common::start(true, &[]).await;
    assert_eq!(get(&base).await.status(), 401);
}
