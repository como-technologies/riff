//! The plan over HTTP (01M4A4YTNSJR0R1T9JNXPBSKHC): the commands `plan`
//! and `plan_off`, the signal `plan_seen` and the query `plan`, on a
//! real server with no sign-in.

use crate::common;

use riff_core::wire::{PlanChanged, PlanOffReply, PlanReply, REFUSED_HEADER};
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

/// The bound of a wait for an event. It only ends a test that hangs.
const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// An open `GET /v1/plan/watch` stream, and the text that it gave and
/// that no event took yet.
struct Changes {
    reply: reqwest::Response,
    buffer: String,
}

impl Changes {
    async fn open(base: &str, thread: &str) -> Changes {
        let reply = common::client()
            .get(format!("{base}/v1/plan/watch"))
            .query(&[("uri", ANN), ("thread", thread)])
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 200);
        Changes {
            reply,
            buffer: String::new(),
        }
    }

    /// The next event, or `None` when none comes in `wait`.
    async fn next(&mut self, wait: std::time::Duration) -> Option<PlanChanged> {
        loop {
            if let Some(end) = self.buffer.find('\n') {
                let line: String = self.buffer.drain(..=end).collect();
                match line.trim().strip_prefix("data:") {
                    Some(data) => return Some(serde_json::from_str(data.trim()).unwrap()),
                    None => continue,
                }
            }
            let chunk = tokio::time::timeout(wait, self.reply.chunk()).await.ok()?;
            let chunk = chunk.unwrap().expect("the stream stays open");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}

/// #1729: each command that changes the free work of a repository thread
/// gives one event on `GET /v1/plan/watch` of that thread: a hold, the
/// free of the hold and the release of a claim. A claim and a post give
/// none, and the stream of another thread gets none.
#[tokio::test]
async fn a_hold_a_free_and_a_release_each_give_one_event_of_the_plan() {
    let (_service, base) = common::start(false, &[]).await;
    let _: Value = ok(&base, "/v1/register", json!({"me": ANN})).await;
    let lead = json!({"me": "riff://ann@heron/acme/app", "session": "a1"});
    let _: Value = ok(&base, "/v1/lead", lead).await;
    let _: Value = ok(&base, "/v1/resume", json!({"me": ANN, "riff": true})).await;
    let mut changes = Changes::open(&base, "acme/app").await;
    let mut other = Changes::open(&base, "acme/web").await;

    let item = json!({"me": ANN, "thread": "acme/app", "item": "issue-12"});
    let mut hold = item.clone();
    hold["reason"] = json!("a reading");
    let _: Value = ok(&base, "/v1/plan/hold", hold).await;
    let held = changes.next(WAIT).await.expect("an event of the hold");
    assert_eq!(held.thread.to_string(), "acme/app");

    let _: Value = ok(&base, "/v1/plan/free", item).await;
    let freed = changes.next(WAIT).await.expect("an event of the free");
    assert!(freed.position > held.position, "{freed:?} {held:?}");

    let claim = json!({"me": ANN, "thread": "acme/app", "item": "issue-13"});
    let _: Value = ok(&base, "/v1/claim", claim.clone()).await;
    let post = json!({"me": ANN, "thread": "acme/app", "to": [], "body": "hi"});
    let _: Value = ok(&base, "/v1/post", post).await;
    let _: Value = ok(&base, "/v1/release", claim).await;
    let released = changes.next(WAIT).await.expect("an event of the release");
    assert!(released.position > freed.position + 1, "{released:?}");
    let short = std::time::Duration::from_millis(300);
    assert_eq!(
        changes.next(short).await,
        None,
        "one event for each command"
    );
    assert_eq!(other.next(short).await, None, "no event of another thread");
}
