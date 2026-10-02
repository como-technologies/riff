//! The import of go-live (01M3Z8MRDZEKTXSKZTDTDSCZ3W,
//! 01M3Z8MRGWWA0CNZ003D67H6R4, 01M3Z8MRKTAN8CBAQB721JNZAK), over HTTP:
//! a start on the objects of a riff-server of v0.8.0 and no log.
//!
//! The objects in `fixtures/0.8.0` come from the code of v0.8.0:
//! `make.rs` there is the program that made them. `facts.json` has what
//! the server of v0.8.0 showed after a restart on its own objects: its
//! `who`, the unread counts and the messages of each thread. It also has
//! the refresh tokens of the sign-ins. The fixture has two
//! repositories, claims, leads, members, messages and read cursors.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use riff_core::build::{Build, HEADER, VERSION};
use riff_core::dpop::Key;
use riff_core::name::SessionUri;
use riff_core::wire::{Post, TokenReply};
use riff_server::store::{Memory, Store};
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

/// A store with the objects of the fixture, saved now. `keys` gives the
/// device key of a sign-in of the fixture, by the name of its key
/// there, for example `jkt-mike`.
async fn old_store(keys: &[(&str, &Key)]) -> Memory {
    let by = now_ms() - facts()["saved_ms"].as_u64().unwrap();
    let store = Memory::default();
    let mut names = vec!["sessions".to_owned(), "tokens".to_owned()];
    for entry in std::fs::read_dir(dir().join("threads")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        names.push(format!("threads/{name}"));
    }
    for name in names {
        let mut text = std::fs::read_to_string(dir().join(&name)).unwrap();
        for (jkt, key) in keys {
            text = text.replace(&format!("\"{jkt}\""), &format!("\"{}\"", key.thumbprint()));
        }
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

/// The seq, the body and the sender of each message of a thread, as
/// `me` reads all of it, page after page.
async fn messages(base: &str, me: &str, thread: &str) -> Vec<Value> {
    let mut all = Vec::new();
    let mut after = Some(0);
    while let Some(seq) = after {
        let read = json!({ "me": me, "thread": thread, "all": true, "after": seq });
        let page = call(base, "read", read).await;
        for m in page["messages"].as_array().unwrap() {
            let from: SessionUri = m["from"].as_str().unwrap().parse().unwrap();
            all.push(json!({ "seq": m["seq"], "body": m["body"], "from": from.who().to_string() }));
        }
        after = page["next"].as_u64();
    }
    all
}

/// The same `who`, the same claims, the same unread counts and the same
/// last messages of each thread as the server of v0.8.0.
#[tokio::test]
async fn the_import_gives_the_state_of_the_old_server() {
    let facts = facts();
    let store = old_store(&[]).await;
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
    let store = old_store(&[]).await;
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
    let token = |name: &str| facts["refresh"][name]["token"].as_str().unwrap().to_owned();
    let (mike, brett, gone) = (Key::generate(), Key::generate(), Key::generate());
    let keys = [
        ("jkt-mike", &mike),
        ("jkt-brett", &brett),
        ("jkt-gone", &gone),
    ];
    let store = old_store(&keys).await;
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
    assert!(refresh(&base, &brett, &token("brett")).await.is_ok());

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
    let store = old_store(&[]).await;
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
