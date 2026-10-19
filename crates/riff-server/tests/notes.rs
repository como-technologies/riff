//! The notes of the lead over HTTP (01M59258K3TYR09HC4AWHWZPEX): the
//! command `notes/set` and the query `notes`, on a real server with no
//! sign-in.

use crate::common;

use riff_core::wire::{NotesReply, REFUSED_HEADER, SetNotesReply};
use serde_json::{Value, json};

const ANN: &str = "riff://ann@heron/acme/app?session=a1";
const WORKER: &str = "riff://ann@heron/acme/app?session=w1";

async fn call(base: &str, path: &str, body: Value) -> reqwest::Response {
    common::client()
        .post(format!("{base}{path}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn ok<T: serde::de::DeserializeOwned>(base: &str, path: &str, body: Value) -> T {
    let reply = call(base, path, body).await;
    assert_eq!(reply.status(), 200, "{path}");
    reply.json().await.unwrap()
}

fn set(text: &str) -> Value {
    json!({"me": ANN, "thread": "acme/app", "text": text})
}

#[tokio::test]
async fn the_notes_go_through_each_route_with_their_history() {
    let (_service, base) = common::start(false, &[]).await;
    let _: Value = ok(&base, "/v1/register", json!({"me": ANN})).await;
    let show = json!({"me": ANN, "thread": "acme/app", "history": true});

    let none: NotesReply = ok(&base, "/v1/notes", show.clone()).await;
    assert_eq!((none.user.as_str(), none.text.as_str()), ("ann", ""));

    let first: SetNotesReply = ok(&base, "/v1/notes/set", set("Wave 36 runs.")).await;
    assert!(first.changed);
    let same: SetNotesReply = ok(&base, "/v1/notes/set", set("Wave 36 runs.")).await;
    assert!(!same.changed);
    let _: SetNotesReply = ok(&base, "/v1/notes/set", set("Wave 37 runs.")).await;

    let now: NotesReply = ok(&base, "/v1/notes", show).await;
    assert_eq!(now.text, "Wave 37 runs.");
    assert!(now.at_ms.is_some());
    let before: Vec<&str> = now.history.iter().map(|v| v.text.as_str()).collect();
    assert_eq!(before, ["Wave 36 runs."]);

    let plain = json!({"me": ANN, "thread": "acme/app"});
    let plain: NotesReply = ok(&base, "/v1/notes", plain).await;
    assert!(plain.history.is_empty());
}

#[tokio::test]
async fn a_text_over_the_limit_or_with_a_token_is_refused() {
    let (_service, base) = common::start(false, &[]).await;
    let _: Value = ok(&base, "/v1/register", json!({"me": ANN})).await;
    let long = call(&base, "/v1/notes/set", set(&"a".repeat(64 * 1024 + 1))).await;
    assert_eq!(long.status(), 400);
    let text = long.text().await.unwrap();
    assert!(text.contains("64 KiB"), "{text}");
    let secret = call(
        &base,
        "/v1/notes/set",
        set("key sk-ant-oat01-abcdefghijklmnop"),
    )
    .await;
    assert_eq!(secret.status(), 400);
    let text = secret.text().await.unwrap();
    assert!(
        text.contains("secret") && !text.contains("sk-ant"),
        "{text}"
    );
    let none: NotesReply = ok(&base, "/v1/notes", json!({"me": ANN, "thread": "acme/app"})).await;
    assert_eq!(none.text, "");
}

#[tokio::test]
async fn a_worker_gets_not_allowed_for_the_notes() {
    let (_service, base) = common::start(false, &[]).await;
    let register = json!({"me": WORKER, "worker": true});
    let _: Value = ok(&base, "/v1/register", register).await;
    let mut send = set("x");
    send["me"] = json!(WORKER);
    let refused = call(&base, "/v1/notes/set", send).await;
    assert_eq!(refused.status(), 403);
    assert_eq!(refused.headers()[REFUSED_HEADER], "not_allowed");
}
