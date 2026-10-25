//! A call that registers a session leaves it there for the next call
//! (#1401). `riff whoami` of a session is a query. The server registers
//! the session before it replies, so the `lead` of the person, a call
//! at once after it, finds the session.

use crate::common;

use futures::future::join_all;
use serde_json::json;

const PERSON: &str = "riff://mike@thelio/como-technologies/riff";

/// The URI of the session `id`.
fn session(id: &str) -> String {
    format!("{PERSON}?session={id}")
}

/// `POST /v1/riff` as the session `id`: the query of `riff whoami`.
async fn whoami(base: &str, id: &str) {
    let reply = common::client()
        .post(format!("{base}/v1/riff"))
        .json(&json!({ "me": session(id) }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "whoami of {id}");
}

/// `POST /v1/lead` as the person: the status and the text of the reply.
async fn lead(base: &str, id: &str) -> (u16, String) {
    let reply = common::client()
        .post(format!("{base}/v1/lead"))
        .json(&json!({ "me": PERSON, "session": id }))
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

#[tokio::test]
async fn the_lead_finds_a_session_at_once_after_its_first_query() {
    let (_service, base) = common::start(false, &[]).await;
    for n in 0..40 {
        let id = format!("lead{n:03}");
        whoami(&base, &id).await;
        let (status, text) = lead(&base, &id).await;
        assert_eq!(status, 200, "{id}: {text}");
    }
}

#[tokio::test]
async fn the_lead_finds_each_of_many_sessions_that_ask_at_the_same_time() {
    let (_service, base) = common::start(false, &[]).await;
    let ids: Vec<String> = (0..60).map(|n| format!("lead{n:03}")).collect();
    let pairs = ids.iter().map(|id| {
        let base = base.clone();
        async move {
            whoami(&base, id).await;
            (id, lead(&base, id).await)
        }
    });
    for (id, (status, text)) in join_all(pairs).await {
        assert_eq!(status, 200, "{id}: {text}");
    }
}

#[tokio::test]
async fn the_lead_finds_no_session_that_never_called() {
    let (_service, base) = common::start(false, &[]).await;
    whoami(&base, "lead001").await;
    let (status, text) = lead(&base, "lead002").await;
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("mike has no session lead002"), "{text}");
}
