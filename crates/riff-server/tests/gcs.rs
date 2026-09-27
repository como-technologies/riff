//! The Cloud Storage store against a fake Cloud Storage and metadata
//! server.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use riff_server::gcs::{Gcs, TOKEN_PATH};
use riff_server::store::{SESSIONS, Store, StoreError, TOKENS, thread_object};
use serde_json::json;

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
    let thread = thread_object(&"como-technologies/riff".parse().unwrap());
    let first = store(&url);
    let sessions = first
        .save(SESSIONS, b"{\"s\":1}".to_vec(), None)
        .await
        .unwrap();
    let saved = first
        .save(&thread, b"{\"t\":1}".to_vec(), None)
        .await
        .unwrap();

    let second = store(&url);
    let loaded = second.load(&thread).await.unwrap().unwrap();
    assert_eq!(loaded.bytes, b"{\"t\":1}");
    assert_eq!(loaded.version, saved);
    assert_eq!(
        second.load(SESSIONS).await.unwrap().unwrap().version,
        sessions
    );
    assert_eq!(second.list("threads/").await.unwrap(), [thread]);
    assert!(second.load(TOKENS).await.unwrap().is_none());
}

#[tokio::test]
async fn a_save_with_an_old_version_fails() {
    let (_fake, url) = start().await;
    let store = store(&url);
    let v1 = store.save(SESSIONS, b"1".to_vec(), None).await.unwrap();
    let v2 = store.save(SESSIONS, b"2".to_vec(), Some(v1)).await.unwrap();
    assert!(v2 > v1);

    let stale = store.save(SESSIONS, b"3".to_vec(), Some(v1)).await;
    assert_eq!(stale, Err(StoreError::Conflict(SESSIONS.into())));
    // A save as new fails too: the object exists.
    let new = store.save(SESSIONS, b"3".to_vec(), None).await;
    assert_eq!(new, Err(StoreError::Conflict(SESSIONS.into())));
    assert_eq!(store.load(SESSIONS).await.unwrap().unwrap().bytes, b"2");
}

#[tokio::test]
async fn a_thread_object_that_the_lifecycle_rule_deleted_is_saved_again() {
    let (fake, url) = start().await;
    let store = store(&url);
    let thread = thread_object(&"a/b".parse().unwrap());
    let v1 = store.save(&thread, b"old".to_vec(), None).await.unwrap();
    fake.lock().unwrap().objects.remove(&thread);

    let v2 = store
        .save(&thread, b"new".to_vec(), Some(v1))
        .await
        .unwrap();
    let loaded = store.load(&thread).await.unwrap().unwrap();
    assert_eq!((loaded.bytes, loaded.version), (b"new".to_vec(), v2));
}

#[tokio::test]
async fn a_changed_thread_object_still_fails() {
    let (_fake, url) = start().await;
    let store = store(&url);
    let thread = thread_object(&"a/b".parse().unwrap());
    let v1 = store.save(&thread, b"1".to_vec(), None).await.unwrap();
    store.save(&thread, b"2".to_vec(), Some(v1)).await.unwrap();

    let stale = store.save(&thread, b"3".to_vec(), Some(v1)).await;
    assert_eq!(stale, Err(StoreError::Conflict(thread.clone())));
    assert_eq!(store.load(&thread).await.unwrap().unwrap().bytes, b"2");
}

#[tokio::test]
async fn only_thread_objects_are_saved_again() {
    let (fake, url) = start().await;
    let store = store(&url);
    let v1 = store.save(SESSIONS, b"1".to_vec(), None).await.unwrap();
    fake.lock().unwrap().objects.remove(SESSIONS);

    let result = store.save(SESSIONS, b"2".to_vec(), Some(v1)).await;
    assert_eq!(result, Err(StoreError::Conflict(SESSIONS.into())));
    assert!(store.load(SESSIONS).await.unwrap().is_none());
}

#[tokio::test]
async fn list_reads_each_page() {
    let (_fake, url) = start().await;
    let store = store(&url);
    let mut threads = Vec::new();
    for i in 0..5 {
        let name = thread_object(&format!("t/{i}").parse().unwrap());
        store.save(&name, vec![], None).await.unwrap();
        threads.push(name);
    }
    store.save(SESSIONS, vec![], None).await.unwrap();

    assert_eq!(store.list("threads/").await.unwrap(), threads);
    assert!(store.list("none/").await.unwrap().is_empty());
}

#[tokio::test]
async fn the_store_keeps_its_access_token() {
    let (fake, url) = start().await;
    let store = store(&url);
    for _ in 0..3 {
        store.save(SESSIONS, vec![], None).await.ok();
        store.load(SESSIONS).await.unwrap();
    }
    assert_eq!(fake.lock().unwrap().token_calls, 1);
}

#[tokio::test]
async fn a_failed_call_is_a_failure_not_a_conflict() {
    let (fake, url) = start().await;
    let store = store(&url);
    fake.lock().unwrap().broken = Some(StatusCode::SERVICE_UNAVAILABLE);

    for result in [
        store.save(SESSIONS, vec![], None).await.map(|_| ()),
        store.load(SESSIONS).await.map(|_| ()),
        store.list("").await.map(|_| ()),
    ] {
        assert!(matches!(result, Err(StoreError::Failed(m)) if m.contains("503")));
    }
}

#[tokio::test]
async fn no_metadata_server_is_a_failure() {
    let (_fake, url) = start().await;
    let store = Gcs::with_urls(BUCKET, &url, "http://127.0.0.1:1");
    let result = store.load(SESSIONS).await;
    assert!(matches!(result, Err(StoreError::Failed(_))), "{result:?}");
}
