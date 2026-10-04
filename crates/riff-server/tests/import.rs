//! The import of go-live (01M3Z8MRDZEKTXSKZTDTDSCZ3W,
//! 01M3Z8MRGWWA0CNZ003D67H6R4, 01M3Z8MRKTAN8CBAQB721JNZAK), over HTTP:
//! a start on the objects of a riff-server of v0.8.0 and no log.
//!
//! The objects in `fixtures/0.8.0` come from the code of v0.8.0:
//! `make.rs` there is the program that made them. `facts.json` has what
//! the server of v0.8.0 showed after a restart on its own objects: its
//! `who`, the unread counts and the messages of each thread. It also has
//! the refresh tokens and the device keys of the sign-ins. The fixture
//! has two repositories, claims, leads, members, messages and read
//! cursors. Its messages have each form of the real riff: signed and
//! not signed, a request of a lead, a status request, and a selector
//! with three fields.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::FutureExt;
use futures::future::BoxFuture;
use riff_core::build::{Build, HEADER, VERSION};
use riff_core::dpop::Key;
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::{Keys, Message, Post, ReadReply, TokenReply};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::lease::Timing;
use riff_server::store::{Loaded, Memory, SIGN_INS, Store, StoreError, Version};
use serde_json::{Value, json};

const RIFF: &str = "como-technologies/riff";
const KEEP: usize = 200;

fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    u64::try_from(since.as_millis()).unwrap()
}

fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/0.8.0")
}

fn facts() -> Value {
    serde_json::from_slice(&std::fs::read(dir().join("facts.json")).unwrap()).unwrap()
}

/// Moves each time of a fixture object by `by` milliseconds: the server
/// of v0.8.0 saved the objects a moment ago.
fn shift(value: &mut Value, by: u64) {
    const TIMES: [&str; 7] = [
        "saved_ms",
        "seen_ms",
        "alive_ms",
        "set_ms",
        "idle_until",
        "expires",
        "until",
    ];
    match value {
        Value::Object(map) => {
            for (name, field) in map.iter_mut() {
                match field.as_u64() {
                    Some(at) if TIMES.contains(&name.as_str()) || name == "used" => {
                        *field = json!(at + by);
                    }
                    _ => shift(field, by),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| shift(item, by)),
        _ => {}
    }
}

/// The device key of a sign-in of the fixture: `mike`, `brett` or
/// `gone`. It signed the messages of its person there.
fn key(name: &str) -> Key {
    Key::from_secret(facts()["keys"][name].as_str().unwrap()).unwrap()
}

/// The keys of the live sign-ins of the fixture, as a `read` gives them.
fn fixture_keys() -> Keys {
    ["mike", "brett"]
        .iter()
        .map(|user| (user.to_string(), vec![key(user).thumbprint()]))
        .collect()
}

/// A store with the objects of the fixture, saved now.
async fn old_store() -> Memory {
    let by = now_ms() - facts()["saved_ms"].as_u64().unwrap();
    let store = Memory::default();
    let mut names = vec!["sessions".to_owned(), "tokens".to_owned()];
    for entry in std::fs::read_dir(dir().join("threads")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        names.push(format!("threads/{name}"));
    }
    for name in names {
        let text = std::fs::read_to_string(dir().join(&name)).unwrap();
        let mut object: Value = serde_json::from_str(&text).unwrap();
        if !name.starts_with("threads/") {
            shift(&mut object, by);
        }
        let bytes = serde_json::to_vec(&object).unwrap();
        store.save(&name, bytes, None).await.unwrap();
    }
    store
}

/// Calls `op` and returns the reply. The call must succeed.
async fn call(base: &str, op: &str, body: Value) -> Value {
    let reply = common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200, "{op} {body}");
    reply.json().await.unwrap()
}

/// The URI of each session, gone sessions too.
async fn who(base: &str) -> BTreeSet<String> {
    let me = "riff://mike@pangolin/como-technologies/riff";
    let who = call(base, "who", json!({ "me": me, "all": true })).await;
    let sessions = who["sessions"].as_array().unwrap();
    sessions
        .iter()
        .filter(|s| s["uri"] != me)
        .map(|s| s["uri"].as_str().unwrap().to_owned())
        .collect()
}

/// The seq, the body, the sender with its lead mark and the kind of
/// each message of a thread, as `me` reads all of it, page after page.
/// `verified` is true when a key of the fixture signed the message as it
/// is.
async fn messages(base: &str, me: &str, thread: &str) -> Vec<Value> {
    // A direct thread has no name that parses from text: read it as JSON.
    let name: ThreadName = serde_json::from_value(json!(thread)).unwrap();
    let keys = fixture_keys();
    let mut all = Vec::new();
    let mut after = Some(0);
    while let Some(seq) = after {
        let read = json!({ "me": me, "thread": thread, "all": true, "after": seq });
        let page = call(base, "read", read).await;
        for m in page["messages"].as_array().unwrap() {
            let m: Message = serde_json::from_value(m.clone()).unwrap();
            all.push(json!({
                "seq": m.seq,
                "body": m.body,
                "from": m.from.who().to_string(),
                "lead": m.from.lead(),
                "kind": m.kind,
                "verified": m.verified(&name, &keys),
            }));
        }
        after = page["next"].as_u64();
    }
    all
}

/// The same `who`, the same claims, the same unread counts and the same
/// last messages of each thread as the server of v0.8.0. A message that
/// was verified there is verified here (01M3ZCDNR0DT9J5XXXS89APTQ2).
#[tokio::test]
async fn the_import_gives_the_state_of_the_old_server() {
    let facts = facts();
    let store = old_store().await;
    let (service, base) = common::start_on(Arc::new(store.clone())).await;

    // The riff keeps its ID, and it is paused.
    assert_eq!(service.riff_id().as_deref(), facts["riff_id"].as_str());
    let m1 = "riff://mike@pangolin/como-technologies/riff?session=m1";
    let b1 = "riff://brett@kadomony/como-technologies/strata?session=b1";

    // The same sessions, each with its place, its lead mark and its
    // claims. No session called yet.
    let old: BTreeSet<String> = facts["who"]
        .as_array()
        .unwrap()
        .iter()
        .map(|uri| uri.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(who(&base).await, old);
    assert!(
        old.iter()
            .any(|uri| uri.contains("session=m2&claim=issue-341"))
    );
    assert!(
        old.iter()
            .any(|uri| uri.contains("session=b2&claim=issue-7"))
    );
    assert!(old.iter().any(|uri| uri.contains("session=m1&lead=true")));

    // The worker marks.
    let me = "riff://mike@pangolin/como-technologies/riff";
    let reply = call(&base, "who", json!({ "me": me, "all": true })).await;
    let workers: BTreeSet<String> = reply["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["worker"] == true)
        .map(|s| {
            let uri: SessionUri = s["uri"].as_str().unwrap().parse().unwrap();
            uri.who().to_string()
        })
        .collect();
    let old_workers: BTreeSet<String> = facts["workers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(workers, old_workers);

    // The same unread counts. A thread keeps its last 200 messages, so a
    // count is at most 200. The direct thread of two sessions that
    // ended is not in the import.
    let ended = "dm:mike/m3|mike/m4";
    let uris: BTreeMap<String, String> = old
        .iter()
        .map(|uri| {
            let parsed: SessionUri = uri.parse().unwrap();
            let plain = SessionUri::new(parsed.who().clone(), parsed.place().clone());
            (parsed.who().to_string(), plain.to_string())
        })
        .collect();
    for (who, threads) in facts["unread"].as_object().unwrap() {
        let reply = call(&base, "threads", json!({ "me": uris[who] })).await;
        let unread: BTreeMap<String, u64> = reply["threads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let thread = t["thread"].as_str().unwrap().to_owned();
                (thread, t["unread"].as_u64().unwrap())
            })
            .collect();
        let old: BTreeMap<String, u64> = threads
            .as_object()
            .unwrap()
            .iter()
            .filter(|(thread, _)| *thread != ended)
            .map(|(thread, n)| (thread.clone(), n.as_u64().unwrap().min(KEEP as u64)))
            .collect();
        assert_eq!(unread, old, "{who}");
    }

    // The same last messages of each thread, with their seq.
    for (thread, old) in facts["messages"].as_object().unwrap() {
        if thread == ended {
            continue;
        }
        let reader = if thread.contains("strata") || thread.starts_with("dm:") {
            b1
        } else {
            m1
        };
        let new = messages(&base, reader, thread).await;
        assert_eq!(&new, old.as_array().unwrap(), "{thread}");
    }
    let riff = messages(&base, m1, RIFF).await;
    assert_eq!(riff.len(), KEEP);
    assert_eq!(
        (riff[0]["seq"].clone(), riff[199]["seq"].clone()),
        (json!(8), json!(207))
    );
    // The fixture has each form: a signed request of a lead, a signed
    // status request, a message with no signature, and a message that
    // the key of no sign-in signed.
    let request = &messages(&base, b1, "dm:brett/b1|brett/b2").await[0];
    assert_eq!(request["body"], "request: claim issue-7");
    assert_eq!(
        (&request["lead"], &request["verified"]),
        (&json!(true), &json!(true))
    );
    let strata = messages(&base, b1, "como-technologies/strata").await;
    let ask = strata.iter().find(|m| m["kind"] == "status").unwrap();
    assert_eq!(ask["verified"], true);
    let design = messages(&base, m1, "design").await;
    assert_eq!(design.len(), 2);
    assert!(design.iter().all(|m| m["verified"] == false), "{design:?}");
    assert_eq!(riff[199]["verified"], true);
    assert_eq!(riff[0]["verified"], false, "a note with no signature");

    // The riff is paused after the import: a claim fails. The owner or
    // an admin resumes the whole riff. The settings stay.
    let claim = json!({ "me": m1, "thread": RIFF, "item": "issue-500" });
    let reply = common::client()
        .post(format!("{base}/v1/claim"))
        .json(&claim)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 409);
    call(&base, "resume", json!({ "me": me, "riff": true })).await;
    let idle = call(&base, "idle", json!({ "me": me })).await;
    assert_eq!(idle, facts["idle"]);

    // A session goes on: the worker keeps its claim, and its next post
    // gets the next seq.
    let m2 = &uris["mike/m2"];
    let post = json!({ "me": m2, "thread": RIFF, "body": "after go-live" });
    assert_eq!(call(&base, "post", post).await["seq"], 208);
    assert!(
        who(&base)
            .await
            .iter()
            .any(|uri| uri.contains("session=m2&claim=issue-341"))
    );
    let held = json!({ "me": m1, "thread": RIFF, "item": "issue-341" });
    let reply = common::client()
        .post(format!("{base}/v1/claim"))
        .json(&held)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 409, "the item is held");
}

/// The import runs one time only. A second start replays the log: it
/// writes no record, and gives the same state.
#[tokio::test]
async fn a_second_start_replays_the_log_and_does_not_import_again() {
    let store = old_store().await;
    let (first, base) = common::start_on(Arc::new(store.clone())).await;
    first.save().await.unwrap();
    let imported = store.list("log/").await.unwrap();
    assert_eq!(imported.len(), 1, "the import is one chunk");
    assert_eq!(store.list("checkpoint/").await.unwrap().len(), 1);
    // The first call of a person makes a record of its own.
    let before = who(&base).await;
    let m2 = "riff://mike@pangolin/como-technologies/riff?session=m2#issue-341";
    let unread = call(&base, "threads", json!({ "me": m2 })).await;
    first.save().await.unwrap();
    let chunks = store.list("log/").await.unwrap();

    let (second, base) = common::start_on(Arc::new(store.clone())).await;
    second.save().await.unwrap();
    assert_eq!(store.list("log/").await.unwrap(), chunks);
    assert_eq!(who(&base).await, before);
    assert_eq!(second.riff_id(), first.riff_id());
    // The read cursors are in the checkpoint of the import.
    assert_eq!(call(&base, "threads", json!({ "me": m2 })).await, unread);
    // The old objects stay, for a rollback.
    for name in ["sessions", "tokens", "threads/design"] {
        assert!(store.load(name).await.unwrap().is_some(), "{name}");
    }
}

const ANN: &str = "riff://ann@heron/acme/app?session=a1";

/// The sessions object of a small riff of v0.8.0: one session of ann
/// with these claims, saved now.
fn small_sessions(claims: &[&str]) -> Vec<u8> {
    let now = now_ms();
    let claims: Vec<Value> = claims
        .iter()
        .map(|item| {
            json!({"thread": "acme/app", "item": item, "who": {"user": "ann", "session": "a1"}})
        })
        .collect();
    serde_json::to_vec(&json!({
        "saved_ms": now,
        "sessions": [{"uri": ANN, "seen_ms": now, "alive_ms": now}],
        "cursors": [],
        "claims": claims,
        "leads": [],
        "riff": "running",
    }))
    .unwrap()
}

/// The thread object of the small riff, with one message for each body.
fn small_thread(bodies: &[&str]) -> Vec<u8> {
    let messages: Vec<Value> = bodies
        .iter()
        .enumerate()
        .map(|(n, body)| {
            let at_ms = now_ms();
            json!({"message": {"seq": n + 1, "from": ANN, "to": [], "body": body, "at_ms": at_ms}})
        })
        .collect();
    serde_json::to_vec(&json!({
        "name": "acme/app",
        "members": [{"user": "ann", "session": "a1"}],
        "messages": messages,
    }))
    .unwrap()
}

/// The time that a slow load waits for the old instance.
const SLOW: Duration = Duration::from_millis(1500);

/// A load that runs, with the listener and the URL of its server.
type Loading = (
    tokio::task::JoinHandle<Result<Service, StoreError>>,
    std::net::TcpListener,
    String,
);

/// Starts the load of a server on `store` that waits [`SLOW`] for the
/// old instance after it takes the lease.
fn load_slow(store: Arc<dyn Store>, require_sign_in: bool) -> Loading {
    common::client();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        require_sign_in,
        lease: Timing {
            wait: SLOW,
            ..common::LEASE
        },
        save_every: common::SAVE_EVERY,
        ..Config::new(&url)
    };
    (tokio::spawn(Service::load(config, store)), listener, url)
}

/// Serves `service` on `listener`.
fn serve(service: &Service, listener: std::net::TcpListener) {
    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
}

/// Copies each object of `from` whose name starts with `prefix` to `to`.
async fn copy(from: &Memory, to: &Memory, prefix: &str) {
    for name in from.list(prefix).await.unwrap() {
        let bytes = from.load(&name).await.unwrap().unwrap().bytes;
        let known = to.load(&name).await.unwrap().map(|l| l.version);
        to.save(&name, bytes, known).await.unwrap();
    }
}

/// The server of v0.8.0 serves and saves its objects until it reads the
/// new lease. The new server reads the old objects again after the
/// wait, so a claim and a message of that time are in the import
/// (01M3ZCDNQY2G9ET537B6SBYCBB).
#[tokio::test]
async fn a_write_of_the_old_server_during_the_lease_wait_is_in_the_import() {
    let store = Memory::default();
    let name = "threads/acme%2Fapp";
    let first = small_sessions(&["issue-7"]);
    store.save("sessions", first, None).await.unwrap();
    store
        .save(name, small_thread(&["first"]), None)
        .await
        .unwrap();
    let (load, listener, base) = load_slow(Arc::new(store.clone()), false);

    // The new server holds the lease now, and waits for the old one.
    tokio::time::sleep(SLOW / 3).await;
    assert!(store.load("lease").await.unwrap().is_some());
    assert!(
        store.list("log/").await.unwrap().is_empty(),
        "no import yet"
    );
    // The old server did not read the lease yet. It takes a claim and a
    // post, and saves, as each second.
    let version = store.load("sessions").await.unwrap().unwrap().version;
    let second = small_sessions(&["issue-7", "issue-8"]);
    store.save("sessions", second, Some(version)).await.unwrap();
    let version = store.load(name).await.unwrap().unwrap().version;
    store
        .save(name, small_thread(&["first", "late"]), Some(version))
        .await
        .unwrap();

    let service = load.await.unwrap().unwrap();
    serve(&service, listener);
    let me = "riff://ann@heron/acme/app";
    let listed = call(&base, "who", json!({ "me": me, "all": true })).await;
    let uris: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.iter()
            .any(|uri| uri.contains("claim=issue-7&claim=issue-8")),
        "{uris:?}"
    );
    let read = json!({ "me": ANN, "thread": "acme/app", "all": true });
    let read = call(&base, "read", read).await;
    let bodies: Vec<&str> = read["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, ["first", "late"]);
}

/// Two new instances start at the same time. The second one reads the
/// old objects before the first one writes the log, and takes the lease
/// after it. The log is there after its wait: it does not import, and
/// it does not replace the sign-ins. So a chain that the first instance
/// gave stays, and a used refresh token of the old server stays used
/// (01M3ZCDNQY2G9ET537B6SBYCBB, 01M3Z8MRKTAN8CBAQB721JNZAK).
#[tokio::test]
async fn a_second_instance_does_not_import_and_keeps_the_sign_ins() {
    let facts = facts();
    let old_token = facts["refresh"]["mike"].as_str().unwrap();
    let mike = key("mike");
    // The first instance imports, and Mike refreshes there.
    let first_store = old_store().await;
    let (first, first_base) = common::start_signed_on(Arc::new(first_store.clone())).await;
    let pair = refresh(&first_base, &mike, old_token).await.unwrap();
    first.save().await.unwrap();

    // The second instance starts on the old objects and no log.
    let store = Memory::default();
    for prefix in ["sessions", "tokens", "threads/"] {
        copy(&first_store, &store, prefix).await;
    }
    let (load, listener, base) = load_slow(Arc::new(store.clone()), true);
    tokio::time::sleep(SLOW / 3).await;
    assert!(store.load("lease").await.unwrap().is_some());
    assert!(store.list("log/").await.unwrap().is_empty());
    // The first instance wrote the log, the checkpoint and the sign-ins
    // until it read the lease of the second one.
    for prefix in ["log/", "checkpoint/", SIGN_INS] {
        copy(&first_store, &store, prefix).await;
    }
    let chunks = store.list("log/").await.unwrap();
    assert!(!chunks.is_empty());

    let second = load.await.unwrap().expect("the second instance serves");
    serve(&second, listener);
    second.save().await.unwrap();
    assert_eq!(
        store.list("log/").await.unwrap(),
        chunks,
        "no second import"
    );
    assert_eq!(second.riff_id(), first.riff_id());
    assert_eq!(second.tokens().sign_ins(), 2);
    // The sign-ins are the sign-ins of the first instance.
    assert_eq!(refresh(&base, &mike, old_token).await.err(), Some(400));
    assert!(refresh(&base, &mike, &pair.refresh_token).await.is_ok());
}

/// A store that refuses each write of the log: a server on it stops
/// between the save of the sign-ins and the write of the log.
struct NoLog(Memory);

impl Store for NoLog {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.0.load(name)
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.0.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        if name.starts_with("log/") {
            return futures::future::ready(Err(StoreError::Conflict(name.into()))).boxed();
        }
        self.0.save(name, bytes, known)
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        self.0.delete(name)
    }
}

/// A server that stops between the save of the sign-ins and the write
/// of the log leaves no log. The next start imports again, and no
/// sign-in is lost (01M3Z8MRGWWA0CNZ003D67H6R4).
#[tokio::test]
async fn a_stop_between_the_sign_ins_and_the_log_loses_no_sign_in() {
    let facts = facts();
    let store = old_store().await;
    let stopped = common::load_on(Arc::new(NoLog(store.clone()))).await;
    assert!(stopped.is_err(), "the import fails with no log");
    assert!(store.load(SIGN_INS).await.unwrap().is_some());
    assert!(store.list("log/").await.unwrap().is_empty());

    let (service, base) = common::start_signed_on(Arc::new(store.clone())).await;
    service.save().await.unwrap();
    assert_eq!(store.list("log/").await.unwrap().len(), 1);
    assert_eq!(service.tokens().sign_ins(), 2);
    for user in ["mike", "brett"] {
        let token = facts["refresh"][user].as_str().unwrap();
        let pair = refresh(&base, &key(user), token).await.unwrap();
        assert_eq!(pair.user, user);
    }
}

/// A refresh at the token endpoint. Gives the pair, or the status.
async fn refresh(base: &str, key: &Key, token: &str) -> Result<TokenReply, u16> {
    let form = format!("grant_type=refresh_token&refresh_token={token}");
    let reply = common::refresh(base, key, &form).await;
    match reply.status().as_u16() {
        200 => Ok(reply.json().await.unwrap()),
        status => Err(status),
    }
}

/// A session with a refresh token of the old server refreshes on the
/// new server with no `riff login`, keeps its claim, and posts
/// (01M3Z8MRGWWA0CNZ003D67H6R4).
#[tokio::test]
async fn a_refresh_token_of_the_old_server_works_one_time_with_no_login() {
    let facts = facts();
    let token = |name: &str| facts["refresh"][name].as_str().unwrap().to_owned();
    let (mike, brett, gone) = (key("mike"), key("brett"), key("gone"));
    let store = old_store().await;
    let (service, base) = common::start_signed_on(Arc::new(store.clone())).await;

    // The owner is the owner of today: no person is the owner by a
    // first sign-in.
    assert_eq!(service.tokens().sign_ins(), 2);
    // A token of another key is refused, and stays good.
    assert_eq!(
        refresh(&base, &brett, &token("mike")).await.err(),
        Some(400)
    );
    // A removed person gets no token.
    assert_eq!(refresh(&base, &gone, &token("gone")).await.err(), Some(400));
    // A used token and a session token of the old server give nothing.
    assert_eq!(
        refresh(&base, &mike, &token("mike_used")).await.err(),
        Some(400)
    );
    assert_eq!(
        refresh(&base, &mike, &token("mike_session")).await.err(),
        Some(400)
    );

    // The person token of today gives a pair of a new chain.
    let person = refresh(&base, &mike, &token("mike")).await.unwrap();
    assert_eq!(person.user, "mike");
    assert_eq!(person.refresh_token.split('.').nth(1), Some("1"));
    // It works one time.
    assert_eq!(refresh(&base, &mike, &token("mike")).await.err(), Some(400));
    let brett_pair = refresh(&base, &brett, &token("brett")).await.unwrap();

    // A worker of Brett reads the request of its lead from before
    // go-live. The server gives the key of the sign-in of Brett, and the
    // message is verified with it: the request counts (01M3ZCDNR0DT9J5XXXS89APTQ2).
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session=b2",
        brett_pair.access_token
    );
    let reply = common::refresh(&base, &brett, &form).await;
    assert_eq!(reply.status(), 200);
    let b2_token: TokenReply = reply.json().await.unwrap();
    let b2 = "riff://brett@kadomony/como-technologies/strata?session=b2#issue-7";
    let direct: ThreadName = serde_json::from_value(json!("dm:brett/b1|brett/b2")).unwrap();
    let read = json!({ "me": b2, "thread": direct, "all": true });
    let url = format!("{base}/v1/read");
    let reply = common::post(&url, &brett, Some(&b2_token.access_token))
        .json(&read)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    let page: ReadReply = reply.json().await.unwrap();
    assert_eq!(page.keys["brett"], [brett.thumbprint()]);
    let [request] = &page.messages[..] else {
        panic!("one request: {:?}", page.messages);
    };
    assert_eq!(request.body, "request: claim issue-7");
    assert!(request.from.lead() && request.verified(&direct, &page.keys));

    // The worker session of Mike swaps the person token for a session
    // token, as each session does.
    let form = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &subject_token={}&session=m2",
        person.access_token
    );
    let reply = common::refresh(&base, &mike, &form).await;
    assert_eq!(reply.status(), 200);
    let session: TokenReply = reply.json().await.unwrap();
    let m2: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=m2#issue-341"
        .parse()
        .unwrap();
    let send = |op: &str, body: Value, token: &str| {
        common::post(&format!("{base}/v1/{op}"), &mike, Some(token))
            .json(&body)
            .send()
    };

    // It keeps its claim.
    let reply = send(
        "who",
        json!({ "me": m2, "all": true }),
        &session.access_token,
    )
    .await
    .unwrap();
    assert_eq!(reply.status(), 200);
    let listed: Value = reply.json().await.unwrap();
    let uris: Vec<&str> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.iter()
            .any(|uri| uri.contains("session=m2&claim=issue-341")),
        "{uris:?}"
    );
    // The owner of today resumes the riff, and the session posts.
    let person_uri = "riff://mike@pangolin/como-technologies/riff";
    let resume = json!({ "me": person_uri, "riff": true });
    let reply = send("resume", resume, &person.access_token).await.unwrap();
    assert_eq!(reply.status(), 200, "mike is the owner");
    let mut post = Post::new(&m2, Some(RIFF.parse().unwrap()), vec![], "after go-live");
    post.sign(&mike, now_ms());
    let reply = send(
        "post",
        serde_json::to_value(&post).unwrap(),
        &session.access_token,
    )
    .await
    .unwrap();
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.json::<Value>().await.unwrap()["seq"], 208);

    // After a restart, the sign-ins of the import stay: the log holds
    // their position and their people.
    service.save().await.unwrap();
    drop(service);
    let (again, base) = common::start_signed_on(Arc::new(store)).await;
    assert_eq!(again.tokens().sign_ins(), 2);
    assert!(refresh(&base, &mike, &person.refresh_token).await.is_ok());
}

/// A riff of v0.8.0 against the new server gets the `riff-build` header
/// in each reply. The header names another build, so the old `riff`
/// updates itself (01M3JEE7P46GWXR1BD4Q1TTSGN).
#[tokio::test]
async fn a_riff_of_v0_8_gets_the_build_header_in_each_reply() {
    let store = old_store().await;
    let (_service, base) = common::start_on(Arc::new(store)).await;
    let old = Build {
        version: "0.8.0".into(),
        commit: "e3cfe5a2919c".into(),
        time: "2026-09-29T21:12:48Z".into(),
    };
    assert_ne!(old.to_string(), VERSION);
    // Each path that a riff of v0.8.0 calls.
    let paths = [
        "admin",
        "alive",
        "claim",
        "end",
        "idle",
        "invite",
        "join",
        "lead",
        "leave",
        "members",
        "owner",
        "owner/deny",
        "owner/take",
        "post",
        "read",
        "register",
        "release",
        "remove",
        "revoke",
        "riff",
        "sign-in",
        "start",
        "status",
        "tail",
        "threads",
        "token",
        "watch",
        "who",
    ];
    let client = reqwest::Client::new();
    for path in paths {
        for get in [false, true] {
            let url = format!("{base}/v1/{path}");
            let request = if get {
                client.get(&url)
            } else {
                client.post(&url).json(&json!({}))
            };
            let reply = request
                .header(HEADER, old.to_string())
                .send()
                .await
                .unwrap();
            let theirs = reply.headers().get(HEADER).map(|v| v.to_str().unwrap());
            assert_eq!(theirs, Some(VERSION), "{path}, status {}", reply.status());
        }
    }
}

/// The idle time of the session of ann in `who --all`.
async fn idle_of_ann(base: &str) -> u64 {
    let me = "riff://ann@heron/acme/app";
    let who = call(base, "who", json!({ "me": me, "all": true })).await;
    let ann = who["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["uri"].as_str().unwrap().contains("session=a1"))
        .unwrap()
        .clone();
    ann["idle_secs"].as_u64().unwrap()
}

/// A session of v0.8.0 last seen 2 days before the import shows 2 days
/// in `who --all`, also after a restart. The records of the import name
/// the session at go-live, but they do not make it newer
/// (01M4263ZXH4K23CSY6C5GJPVQH).
#[tokio::test]
async fn the_seen_time_of_an_old_session_stays_after_a_restart() {
    const TWO_DAYS: u64 = 2 * 24 * 60 * 60;
    let two_days = TWO_DAYS..TWO_DAYS + 60;
    let seen = now_ms() - TWO_DAYS * 1000;
    let store = Memory::default();
    let sessions = json!({
        "saved_ms": now_ms(),
        "sessions": [{"uri": ANN, "seen_ms": seen, "alive_ms": seen}],
        "cursors": [],
        "claims": [],
        "leads": [],
        "riff": "running",
    });
    let sessions = serde_json::to_vec(&sessions).unwrap();
    store.save("sessions", sessions, None).await.unwrap();
    let thread = small_thread(&["first"]);
    store.save("threads/acme%2Fapp", thread, None).await.unwrap();

    let (first, base) = common::start_on(Arc::new(store.clone())).await;
    let after_import = idle_of_ann(&base).await;
    assert!(two_days.contains(&after_import), "{after_import}");
    first.shutdown().await.unwrap();
    let (second, base) = common::start_on(Arc::new(store.clone())).await;
    let after_restart = idle_of_ann(&base).await;
    assert!(two_days.contains(&after_restart), "{after_restart}");

    // A start from the checkpoint of the import alone gives the same.
    second.shutdown().await.unwrap();
    let checkpoints = store.list("checkpoint/").await.unwrap();
    assert!(checkpoints.len() > 1, "{checkpoints:?}");
    for name in checkpoints.iter().skip(1) {
        store.delete(name).await.unwrap();
    }
    let (_third, base) = common::start_on(Arc::new(store)).await;
    let from_import = idle_of_ann(&base).await;
    assert!(two_days.contains(&from_import), "{from_import}");
}
