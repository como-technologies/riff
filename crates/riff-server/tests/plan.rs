//! The plan over HTTP (01M4A4YTNSJR0R1T9JNXPBSKHC): the commands `plan`
//! and `plan_off`, the signal `plan_seen` and the query `plan`, on a
//! real server with no sign-in.

use crate::common;

use riff_core::wire::{PlanOffReply, PlanReply, REFUSED_HEADER};
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

fn plan(base: Option<u64>, items: &[&str]) -> Value {
    let items: Vec<Value> = items
        .iter()
        .map(|item| json!({"item": item, "needs": []}))
        .collect();
    json!({
        "me": ANN,
        "base": base,
        "plan": {
            "thread": "acme/app",
            "wave": {"number": 21, "title": "Wave 21"},
            "items": items,
            "done": [],
        },
    })
}

#[tokio::test]
async fn a_plan_goes_through_each_route() {
    let (_service, base) = common::start(false, &[]).await;
    let _: Value = ok(&base, "/v1/register", json!({"me": ANN})).await;
    let thread = json!({"me": ANN, "thread": "acme/app"});

    // No plan yet: the query gives an empty reply.
    let none: PlanReply = ok(&base, "/v1/plan/show", thread.clone()).await;
    assert_eq!(none, PlanReply::default());

    let set: PlanReply = ok(&base, "/v1/plan", plan(None, &["issue-12"])).await;
    let shown = set.plan.unwrap();
    assert_eq!(shown.plan.items[0].item, "issue-12");
    assert!(shown.seen_ms.is_some() && !shown.stale);
    let position = shown.position;

    // A second plan with the old base crosses: stale_base, 409.
    let crossed = call(&base, "/v1/plan", plan(None, &["issue-13"])).await;
    assert_eq!(crossed.status(), 409);
    assert_eq!(crossed.headers()[REFUSED_HEADER], "stale_base");
    let text = crossed.text().await.unwrap();
    assert!(text.contains(&format!("at position {position}")), "{text}");

    // plan_seen of the position of the server gives the same plan.
    let seen = json!({"me": ANN, "thread": "acme/app", "position": position});
    let seen: PlanReply = ok(&base, "/v1/plan/seen", seen).await;
    assert_eq!(seen.plan.unwrap().position, position);
    let shown: PlanReply = ok(&base, "/v1/plan/show", thread.clone()).await;
    assert_eq!(shown.plan.unwrap().plan.items[0].item, "issue-12");

    let off: PlanOffReply = ok(&base, "/v1/plan/off", thread.clone()).await;
    assert!(off.ended);
    let again: PlanOffReply = ok(&base, "/v1/plan/off", thread.clone()).await;
    assert!(!again.ended);
    let gone: PlanReply = ok(&base, "/v1/plan/show", thread).await;
    assert_eq!(gone.plan, None);
}

#[tokio::test]
async fn a_worker_gets_not_allowed_for_a_plan() {
    let (_service, base) = common::start(false, &[]).await;
    let register = json!({"me": WORKER, "worker": true});
    let _: Value = ok(&base, "/v1/register", register).await;
    let mut send = plan(None, &["issue-12"]);
    send["me"] = json!(WORKER);
    let refused = call(&base, "/v1/plan", send).await;
    assert_eq!(refused.status(), 403);
    assert_eq!(refused.headers()[REFUSED_HEADER], "not_allowed");
    let off = json!({"me": WORKER, "thread": "acme/app"});
    let refused = call(&base, "/v1/plan/off", off).await;
    assert_eq!(refused.headers()[REFUSED_HEADER], "not_allowed");
}
