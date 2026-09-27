//! The state after a restart (R30, R124, R125, R141), over HTTP.

mod common;

use std::sync::Arc;

use riff_server::store::{Memory, StoreError};
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
    let who = call(&base, "who", json!({})).await;
    let threads = call(&base, "threads", json!({ "me": BRETT })).await;
    old.save().await.unwrap();

    let (_new, base) = common::start_on(Arc::new(store)).await;
    assert_eq!(call(&base, "who", json!({})).await, who);
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
async fn a_save_over_the_changes_of_another_server_fails() {
    let store = Memory::default();
    let (old, old_base) = common::start_on(Arc::new(store.clone())).await;
    call(&old_base, "register", json!({ "me": MIKE })).await;
    old.save().await.unwrap();

    let (new, new_base) = common::start_on(Arc::new(store)).await;
    let post = json!({ "me": BRETT, "thread": REPO, "body": "new" });
    call(&new_base, "post", post).await;
    new.save().await.unwrap();

    let post = json!({ "me": MIKE, "thread": REPO, "body": "old" });
    call(&old_base, "post", post).await;
    let error = old.save().await.unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)), "{error}");
    // The object stays changed, so the next save fails too.
    assert!(old.save().await.is_err());
}
