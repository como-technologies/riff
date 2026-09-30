//! `GET /v1/me` gives only the session of the caller: its state, its
//! claims, its status and the build of the server. It changes nothing
//! (01M3T5GFVS8NMA992KHZN4VE17).

mod common;

use riff_core::wire::{MeReply, SessionState, WhoReply};
use serde_json::{Value, json};

const A: &str = "riff://mike@pangolin/como-technologies/riff?session=a";
const B: &str = "riff://brett@kadomony/como-technologies/riff?session=b";
const C: &str = "riff://mike@pangolin/como-technologies/riff?session=c";

/// `POST /v1/OP` with `body`. Returns the reply.
async fn call(base: &str, op: &str, body: Value) -> Value {
    let reply = common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{op}");
    reply.json().await.unwrap()
}

/// `GET /v1/me` for `uri`.
async fn me(base: &str, uri: &str) -> MeReply {
    let reply = common::client()
        .get(format!("{base}/v1/me"))
        .query(&[("uri", uri)])
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    reply.json().await.unwrap()
}

#[tokio::test]
async fn me_gives_the_state_claims_and_status_of_the_caller_only() {
    let (_service, base) = common::start(false, &[]).await;
    for uri in [A, B] {
        call(&base, "register", json!({ "me": uri })).await;
    }
    call(&base, "riff", json!({ "me": A, "state": "running" })).await;
    let thread = "como-technologies/riff";
    for (uri, item) in [(A, "issue-1"), (B, "issue-2")] {
        let reply = call(
            &base,
            "claim",
            json!({ "me": uri, "thread": thread, "item": item }),
        )
        .await;
        assert_eq!(reply["granted"], true);
    }
    let status = json!({ "step": "tests", "blocked": "waits for a verify" });
    call(&base, "status", json!({ "me": A, "status": status })).await;
    call(
        &base,
        "status",
        json!({ "me": B, "status": { "step": "docs" } }),
    )
    .await;

    let reply = me(&base, A).await;
    let session = reply.session.expect("the server knows A");
    assert_eq!(session.uri.who().session(), Some("a"));
    assert_eq!(session.uri.claims(), ["issue-1"]);
    let status = session.status.expect("A has a status");
    assert_eq!(status.status.step, "tests");
    assert_eq!(status.status.blocked.as_deref(), Some("waits for a verify"));
    // A has no open watch stream.
    assert_eq!(session.state, Some(SessionState::Offline));
    assert_eq!(reply.build, riff_core::build::VERSION);

    // The reply holds no other session.
    let text = common::client()
        .get(format!("{base}/v1/me"))
        .query(&[("uri", A)])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !text.contains("issue-2") && !text.contains("brett"),
        "{text}"
    );
}

#[tokio::test]
async fn me_of_an_unknown_session_is_none_and_adds_no_session() {
    let (_service, base) = common::start(false, &[]).await;
    call(&base, "register", json!({ "me": A })).await;

    assert!(me(&base, C).await.session.is_none());

    let who: WhoReply =
        serde_json::from_value(call(&base, "who", json!({ "me": A, "all": true })).await).unwrap();
    let ids: Vec<_> = who.sessions.iter().map(|s| s.uri.who().session()).collect();
    assert_eq!(ids, [Some("a")]);
}
