//! A request to run one command outside the profile, over HTTP (#614,
//! 01M4DA9PFR6V3K3FE1568277H3, 01M4DA9PJ0MJPBQRTA79CVXEA2,
//! 01M4DA9PM89KP332T6BR7V0CDT, 01M4DA9PPFBVPHP57JZQDF4R7R).

use crate::common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{OutsideRequest, OutsideRequests, OutsideState, ReadReply};
use riff_server::Service;
use serde_json::{Value, json};

const SESSION: &str = "riff://mike@pangolin/acme/app?session=a6cf";
const OTHER: &str = "riff://mike@pangolin/acme/app?session=b7d0";
const MIKE: &str = "riff://mike@pangolin/acme/app";
const DAN: &str = "riff://dan@kadomony/acme/app";

/// A caller with a device key and a token.
struct Caller {
    key: Key,
    token: String,
}

impl Caller {
    /// `email` signs in: a token of the person. The first person is the
    /// owner of the riff.
    async fn person(service: &Service, email: &str) -> Caller {
        let key = Key::generate();
        let token = service
            .admit(email, true, &key.thumbprint())
            .await
            .unwrap()
            .access_token;
        Caller { key, token }
    }

    /// A token of the session `id` of the person `self`.
    fn session(&self, service: &Service, id: &str) -> Caller {
        let token = service
            .tokens()
            .for_session(&self.token, &self.key.thumbprint(), id, Instant::now())
            .unwrap()
            .access_token;
        Caller {
            key: self.key.clone(),
            token,
        }
    }

    async fn call(&self, base: &str, op: &str, body: Value) -> (u16, String) {
        let reply = common::post(&format!("{base}/v1/{op}"), &self.key, Some(&self.token))
            .json(&body)
            .send()
            .await
            .unwrap();
        (reply.status().as_u16(), reply.text().await.unwrap())
    }

    async fn ok<T: serde::de::DeserializeOwned>(&self, base: &str, op: &str, body: Value) -> T {
        let (status, text) = self.call(base, op, body).await;
        assert_eq!(status, 200, "{op}: {text}");
        serde_json::from_str(&text).unwrap()
    }
}

#[tokio::test]
async fn an_admin_approves_and_the_session_that_asked_takes_it_one_time() {
    let (service, base) = common::start(true, &[]).await;
    // mike is the owner; dan is a member, not an admin.
    let mike = Caller::person(&service, "mike@comotechnologies.io").await;
    let dan = Caller::person(&service, "dan@comotechnologies.io").await;
    let session = mike.session(&service, "a6cf");
    let other = mike.session(&service, "b7d0");
    session
        .ok::<Value>(&base, "register", json!({ "me": SESSION }))
        .await;

    let ask = json!({
        "me": SESSION,
        "command": ["sudo", "true"],
        "cwd": "/w/issue-1",
        "reason": "the test needs root",
    });
    let asked: OutsideRequest = session.ok(&base, "outside/ask", ask).await;
    assert_eq!(asked.state, OutsideState::Asked);
    assert_eq!(asked.by.to_string(), SESSION);
    let id = asked.id.clone();

    // A person asks nothing: only a session asks.
    let (status, text) = mike
        .call(
            &base,
            "outside/ask",
            json!({ "me": MIKE, "command": ["true"], "cwd": "/w", "reason": "r" }),
        )
        .await;
    assert_eq!(status, 400, "{text}");

    // No session approves: not the session that asked, not another.
    let approve = |me: &str| json!({ "me": me, "id": id, "approve": true });
    for (caller, me) in [(&session, SESSION), (&other, OTHER)] {
        let (status, text) = caller.call(&base, "outside/decide", approve(me)).await;
        assert_eq!(status, 403, "{me}: {text}");
        assert!(text.contains("a session cannot approve"), "{text}");
    }
    // A member that is not an admin does not approve or list.
    let (status, text) = dan.call(&base, "outside/decide", approve(DAN)).await;
    assert_eq!(status, 403, "{text}");
    let (status, _) = dan.call(&base, "outside/list", json!({ "me": DAN })).await;
    assert_eq!(status, 403);

    // The broker of the session waits while no admin approved.
    let take = |me: &str| json!({ "me": me, "id": id });
    let waits: OutsideRequest = session.ok(&base, "outside/take", take(SESSION)).await;
    assert!(!waits.taken);
    assert_eq!(waits.state, OutsideState::Asked);

    let list: OutsideRequests = mike.ok(&base, "outside/list", json!({ "me": MIKE })).await;
    assert_eq!(list.requests, [asked.clone()]);
    let approved: OutsideRequest = mike.ok(&base, "outside/decide", approve(MIKE)).await;
    assert_eq!(approved.state, OutsideState::Approved);
    assert_eq!(approved.decided_by.as_deref(), Some("mike"));

    // Only the session that asked takes it, and only one time.
    let (status, text) = other.call(&base, "outside/take", take(OTHER)).await;
    assert_eq!(status, 400, "{text}");
    let taken: OutsideRequest = session.ok(&base, "outside/take", take(SESSION)).await;
    assert!(taken.taken);
    assert_eq!(taken.command, ["sudo", "true"]);
    assert_eq!(taken.cwd, "/w/issue-1");
    let again: OutsideRequest = session.ok(&base, "outside/take", take(SESSION)).await;
    assert!(!again.taken);
    assert_eq!(again.state, OutsideState::Ran);

    // The thread of the repository holds the log of each step: who
    // asked, who approved, the command and the reason.
    let read = json!({ "me": SESSION, "thread": "acme/app", "all": true });
    let read: ReadReply = session.ok(&base, "read", read).await;
    let bodies: Vec<&str> = read
        .messages
        .iter()
        .map(|m| m.body.as_str())
        .filter(|b| b.starts_with(&format!("outside {id}")))
        .collect();
    assert_eq!(bodies.len(), 3, "{bodies:#?}");
    for body in &bodies {
        assert!(body.contains("mike/a6cf asks to run `sudo true` in /w/issue-1"), "{body}");
        assert!(body.contains("reason: the test needs root"), "{body}");
    }
    assert!(bodies[1].contains("approved by mike"), "{}", bodies[1]);
    assert!(bodies[2].contains("runs it one time"), "{}", bodies[2]);
}

#[tokio::test]
async fn an_admin_denies_and_the_request_never_runs() {
    let (service, base) = common::start(true, &[]).await;
    let mike = Caller::person(&service, "mike@comotechnologies.io").await;
    let session = mike.session(&service, "a6cf");
    let ask = json!({ "me": SESSION, "command": ["true"], "cwd": "/w", "reason": "r" });
    let asked: OutsideRequest = session.ok(&base, "outside/ask", ask).await;
    let deny = json!({ "me": MIKE, "id": asked.id, "approve": false });
    let denied: OutsideRequest = mike.ok(&base, "outside/decide", deny.clone()).await;
    assert_eq!(denied.state, OutsideState::Denied);
    let take = json!({ "me": SESSION, "id": asked.id });
    let took: OutsideRequest = session.ok(&base, "outside/take", take).await;
    assert!(!took.taken);
    assert_eq!(took.state, OutsideState::Denied);
    // A decision is final.
    let (status, text) = mike.call(&base, "outside/decide", deny).await;
    assert_eq!(status, 403, "{text}");
}
