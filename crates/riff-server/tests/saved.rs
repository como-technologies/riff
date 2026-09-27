//! The state after a restart (R30, R124, R125, R141), over HTTP.

mod common;

use std::sync::Arc;

use riff_server::store::{Memory, SESSIONS, Store, StoreError};
use serde_json::{Value, json};

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b#tests";
const REPO: &str = "como-technologies/riff";

/// Calls `op` and returns the reply. The call must succeed.
async fn call(base: &str, op: &str, body: Value) -> Value {
    let reply = reqwest::Client::new()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{op}");
    reply.json().await.unwrap()
}

/// The URI and the live flag of each session in a `who` reply. The idle
/// time can change over a restart.
fn uris(who: serde_json::Value) -> Vec<(serde_json::Value, serde_json::Value)> {
    let sessions = who["sessions"].as_array().unwrap();
    sessions
        .iter()
        .map(|s| (s["uri"].clone(), s["live"].clone()))
        .collect()
}

#[tokio::test]
async fn a_new_server_on_the_same_store_has_the_same_state() {
    let store = Memory::default();
    let (old, base) = common::start_on(Arc::new(store.clone())).await;
    call(&base, "register", json!({ "me": MIKE })).await;
    call(&base, "register", json!({ "me": BRETT })).await;
    let to_brett = json!([{ "user": "brett" }]);
    let post = json!({ "me": MIKE, "thread": "design", "to": to_brett, "body": "look" });
    call(&base, "post", post).await;
    let post = json!({ "me": MIKE, "thread": REPO, "body": "one" });
    call(&base, "post", post).await;
    call(&base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    let claim = json!({ "me": BRETT, "thread": REPO, "item": "issue-6" });
    call(&base, "claim", claim).await;
    let who = uris(call(&base, "who", json!({ "me": MIKE })).await);
    let threads = call(&base, "threads", json!({ "me": BRETT })).await;
    old.save().await.unwrap();

    let (_new, base) = common::start_on(Arc::new(store)).await;
    assert_eq!(uris(call(&base, "who", json!({ "me": MIKE })).await), who);
    assert_eq!(
        call(&base, "threads", json!({ "me": BRETT })).await,
        threads
    );
    let read = call(&base, "read", json!({ "me": BRETT, "thread": "design" })).await;
    assert_eq!(read["messages"][0]["body"], "look");
    // Brett read the repository thread before the restart.
    let read = call(&base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    assert_eq!(read["messages"], json!([]));
    // Brett still holds the claim.
    let claim = json!({ "me": MIKE, "thread": REPO, "item": "issue-6" });
    let reply = call(&base, "claim", claim).await;
    assert_eq!(reply["granted"], false);
    assert!(
        reply["holder"]
            .as_str()
            .unwrap()
            .starts_with("riff://brett@")
    );
}

#[tokio::test]
async fn a_save_that_finds_another_version_stops_the_server() {
    let store = Memory::default();
    let (server, base) = common::start_on(Arc::new(store.clone())).await;
    call(&base, "register", json!({ "me": MIKE })).await;
    server.save().await.unwrap();
    // Another instance writes the sessions object.
    let known = store.load(SESSIONS).await.unwrap().unwrap().version;
    store
        .save(SESSIONS, b"{}".to_vec(), Some(known))
        .await
        .unwrap();

    let post = json!({ "me": MIKE, "thread": REPO, "body": "lost" });
    call(&base, "post", post).await;
    let error = server.save().await.unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)), "{error}");
    // The server stopped for good (R141): 503, and no more saves.
    server.stopped().await;
    let reply = reqwest::Client::new()
        .post(format!("{base}/v1/who"))
        .json(&json!({ "me": MIKE }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 503);
    let post = json!({ "me": MIKE, "thread": REPO, "body": "also lost" });
    let _ = reqwest::Client::new()
        .post(format!("{base}/v1/post"))
        .json(&post)
        .send()
        .await;
    server.save().await.unwrap();
    assert_eq!(store.load(SESSIONS).await.unwrap().unwrap().bytes, b"{}");
}
