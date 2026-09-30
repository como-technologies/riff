//! The Cloud Storage store against a fake Cloud Storage and metadata
//! server.

mod common;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use riff_server::gcs::{Gcs, TOKEN_PATH};
use riff_server::log::chunk_name;
use riff_server::store::{LEASE, Store, StoreError, TOKENS};
use serde_json::{Value, json};

const BUCKET: &str = "riff-test";
const TOKEN: &str = "fake-token";
/// The fake gives at most this many names on one page.
const PAGE: usize = 2;

#[derive(Default)]
struct Fake {
    objects: BTreeMap<String, (Vec<u8>, u64)>,
    generation: u64,
    token_calls: usize,
    /// When set, each storage call gets this status.
    broken: Option<StatusCode>,
}

type Shared = Arc<Mutex<Fake>>;

/// The reply that refuses the call, or `None` when the call may go on.
fn denied(fake: &Shared, headers: &HeaderMap) -> Option<Response> {
    if let Some(status) = fake.lock().unwrap().broken {
        return Some((status, "broken").into_response());
    }
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    if auth == Some(&format!("Bearer {TOKEN}")) {
        None
    } else {
        Some(StatusCode::UNAUTHORIZED.into_response())
    }
}

async fn token(State(fake): State<Shared>, headers: HeaderMap) -> Response {
    if headers.get("metadata-flavor").and_then(|v| v.to_str().ok()) != Some("Google") {
        return StatusCode::FORBIDDEN.into_response();
    }
    fake.lock().unwrap().token_calls += 1;
    Json(json!({"access_token": TOKEN, "expires_in": 3599, "token_type": "Bearer"})).into_response()
}

async fn object(
    State(fake): State<Shared>,
    Path((bucket, name)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(denied) = denied(&fake, &headers) {
        return denied;
    }
    assert_eq!(bucket, BUCKET);
    assert_eq!(query.get("alt").map(String::as_str), Some("media"));
    match fake.lock().unwrap().objects.get(&name) {
        Some((bytes, generation)) => (
            [("x-goog-generation", generation.to_string())],
            bytes.clone(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn list(
    State(fake): State<Shared>,
    Path(bucket): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(denied) = denied(&fake, &headers) {
        return denied;
    }
    assert_eq!(bucket, BUCKET);
    let prefix = query.get("prefix").cloned().unwrap_or_default();
    let after = query.get("pageToken").cloned().unwrap_or_default();
    let fake = fake.lock().unwrap();
    let names: Vec<&String> = fake
        .objects
        .keys()
        .filter(|name| name.starts_with(&prefix) && **name > after)
        .collect();
    let page: Vec<_> = names
        .iter()
        .take(PAGE)
        .map(|n| json!({"name": n}))
        .collect();
    let mut reply = json!({"kind": "storage#objects"});
    if !page.is_empty() {
        reply["items"] = json!(page);
    }
    if names.len() > PAGE {
        reply["nextPageToken"] = json!(names[PAGE - 1]);
    }
    Json(reply).into_response()
}

async fn upload(
    State(fake): State<Shared>,
    Path(bucket): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(denied) = denied(&fake, &headers) {
        return denied;
    }
    assert_eq!(bucket, BUCKET);
    assert_eq!(query.get("uploadType").map(String::as_str), Some("media"));
    let name = query["name"].clone();
    let wanted: u64 = query["ifGenerationMatch"].parse().unwrap();
    let mut fake = fake.lock().unwrap();
    let current = fake.objects.get(&name).map_or(0, |(_, g)| *g);
    if current != wanted {
        return StatusCode::PRECONDITION_FAILED.into_response();
    }
    fake.generation += 1;
    let generation = fake.generation;
    fake.objects
        .insert(name.clone(), (body.to_vec(), generation));
    // Cloud Storage sends the generation as a string.
    Json(json!({"name": name, "generation": generation.to_string()})).into_response()
}

/// Starts the fake. Returns its state and its URL.
async fn start() -> (Shared, String) {
    let fake = Shared::default();
    let router = Router::new()
        .route(TOKEN_PATH, get(token))
        .route("/storage/v1/b/{bucket}/o", get(list))
        .route("/storage/v1/b/{bucket}/o/{name}", get(object))
        .route("/upload/storage/v1/b/{bucket}/o", post(upload))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (fake, url)
}

fn store(url: &str) -> Gcs {
    Gcs::with_urls(BUCKET, url, url)
}

#[tokio::test]
async fn a_new_store_on_the_same_bucket_loads_the_saved_objects() {
    let (_fake, url) = start().await;
    let chunk = chunk_name(1);
    let first = store(&url);
    let lease = first
        .save(LEASE, b"{\"s\":1}".to_vec(), None)
        .await
        .unwrap();
    let saved = first
        .save(&chunk, b"{\"t\":1}".to_vec(), None)
        .await
        .unwrap();

    let second = store(&url);
    let loaded = second.load(&chunk).await.unwrap().unwrap();
    assert_eq!(loaded.bytes, b"{\"t\":1}");
    assert_eq!(loaded.version, saved);
    assert_eq!(second.load(LEASE).await.unwrap().unwrap().version, lease);
    assert_eq!(second.list("log/").await.unwrap(), [chunk]);
    assert!(second.load(TOKENS).await.unwrap().is_none());
}

#[tokio::test]
async fn a_save_with_an_old_version_fails() {
    let (_fake, url) = start().await;
    let store = store(&url);
    let v1 = store.save(TOKENS, b"1".to_vec(), None).await.unwrap();
    let v2 = store.save(TOKENS, b"2".to_vec(), Some(v1)).await.unwrap();
    assert!(v2 > v1);

    let stale = store.save(TOKENS, b"3".to_vec(), Some(v1)).await;
    assert_eq!(stale, Err(StoreError::Conflict(TOKENS.into())));
    // A save as new fails too: the object exists.
    let new = store.save(TOKENS, b"3".to_vec(), None).await;
    assert_eq!(new, Err(StoreError::Conflict(TOKENS.into())));
    assert_eq!(store.load(TOKENS).await.unwrap().unwrap().bytes, b"2");
}

#[tokio::test]
async fn a_chunk_never_replaces_a_chunk() {
    let (fake, url) = start().await;
    let store = store(&url);
    let chunk = chunk_name(7);
    store.save(&chunk, b"first".to_vec(), None).await.unwrap();
    let again = store.save(&chunk, b"second".to_vec(), None).await;
    assert_eq!(again, Err(StoreError::Conflict(chunk.clone())));
    assert_eq!(store.load(&chunk).await.unwrap().unwrap().bytes, b"first");
    // A deleted object is not written again as a side effect.
    fake.lock().unwrap().objects.remove(TOKENS);
    let v1 = store.save(TOKENS, b"1".to_vec(), None).await.unwrap();
    fake.lock().unwrap().objects.remove(TOKENS);
    let result = store.save(TOKENS, b"2".to_vec(), Some(v1)).await;
    assert_eq!(result, Err(StoreError::Conflict(TOKENS.into())));
}

#[tokio::test]
async fn list_reads_each_page() {
    let (_fake, url) = start().await;
    let store = store(&url);
    let mut chunks = Vec::new();
    for i in 1..=5 {
        let name = chunk_name(i);
        store.save(&name, vec![], None).await.unwrap();
        chunks.push(name);
    }
    store.save(TOKENS, vec![], None).await.unwrap();

    assert_eq!(store.list("log/").await.unwrap(), chunks);
    assert!(store.list("none/").await.unwrap().is_empty());
}

#[tokio::test]
async fn the_store_keeps_its_access_token() {
    let (fake, url) = start().await;
    let store = store(&url);
    for _ in 0..3 {
        store.save(LEASE, vec![], None).await.ok();
        store.load(LEASE).await.unwrap();
    }
    assert_eq!(fake.lock().unwrap().token_calls, 1);
}

#[tokio::test]
async fn a_failed_call_is_a_failure_not_a_conflict() {
    let (fake, url) = start().await;
    let store = store(&url);
    fake.lock().unwrap().broken = Some(StatusCode::SERVICE_UNAVAILABLE);

    for result in [
        store.save(LEASE, vec![], None).await.map(|_| ()),
        store.load(LEASE).await.map(|_| ()),
        store.list("").await.map(|_| ()),
    ] {
        assert!(matches!(result, Err(StoreError::Failed(m)) if m.contains("503")));
    }
}

#[tokio::test]
async fn no_metadata_server_is_a_failure() {
    let (_fake, url) = start().await;
    let store = Gcs::with_urls(BUCKET, &url, "http://127.0.0.1:1");
    let result = store.load(LEASE).await;
    assert!(matches!(result, Err(StoreError::Failed(_))), "{result:?}");
}

/// Calls `op` on a riff server. The call must succeed.
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

/// The URI and the live flag of each session in a `who` reply.
fn uris(who: serde_json::Value) -> Vec<(serde_json::Value, serde_json::Value)> {
    let sessions = who["sessions"].as_array().unwrap();
    sessions
        .iter()
        .map(|s| (s["uri"].clone(), s["live"].clone()))
        .collect()
}

#[tokio::test]
async fn a_new_server_on_the_same_bucket_has_the_same_state() {
    let (fake, url) = start().await;
    let mike = "riff://mike@pangolin/como-technologies/riff?session=a";
    let brett = "riff://brett@heron/como-technologies/riff?session=b";
    let repo = "como-technologies/riff";
    let (old, base) = common::start_on(Arc::new(store(&url))).await;
    call(&base, "register", json!({ "me": mike })).await;
    call(&base, "register", json!({ "me": brett })).await;
    // Mike's session is the lead: it resumes the new riff.
    call(&base, "riff", json!({ "me": mike, "state": "running" })).await;
    let post = json!({ "me": mike, "thread": repo, "body": "saved" });
    call(&base, "post", post).await;
    let claim = json!({ "me": brett, "thread": repo, "item": "issue-44" });
    call(&base, "claim", claim).await;
    let all = json!({ "me": mike, "all": true });
    let who = uris(call(&base, "who", all.clone()).await);
    old.save().await.unwrap();
    assert!(fake.lock().unwrap().objects.contains_key(&chunk_name(1)));

    let (_new, base) = common::start_on(Arc::new(store(&url))).await;
    assert_eq!(uris(call(&base, "who", all).await), who);
    let state = call(&base, "riff", json!({ "me": mike })).await;
    assert_eq!(state["state"], "running", "the riff state stays");
    let read = call(&base, "read", json!({ "me": brett, "thread": repo })).await;
    assert_eq!(read["messages"][0]["body"], "saved");
    let claim = json!({ "me": mike, "thread": repo, "item": "issue-44" });
    assert_eq!(call(&base, "claim", claim).await["granted"], false);
}

/// A token store from before the owner and members change: it has no
/// `users` field.
const OLD_TOKENS: &str = r#"{"next_sign_in":0,"sign_ins":[],"access":[],"refresh":[]}"#;

#[tokio::test]
async fn a_token_store_of_an_old_format_stops_the_load_with_the_fix() {
    let (_fake, url) = start().await;
    let old = store(&url);
    old.save(TOKENS, OLD_TOKENS.into(), None).await.unwrap();

    let error = common::load_on(Arc::new(store(&url))).await.err().unwrap();
    assert!(matches!(error, StoreError::NotValid { .. }), "{error:?}");
    assert_eq!(
        error.to_string(),
        "cannot read the saved object gs://riff-test/tokens: missing field `users` \
         at line 1 column 57. It can be state of an old format. To start again with an \
         empty state, stop each server of this store and remove the old state: \
         gcloud storage rm 'gs://riff-test/**'"
    );
}
