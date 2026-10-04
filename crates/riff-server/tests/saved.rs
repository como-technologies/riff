//! The state after a restart: a replay of the log (R30, R125, R141),
//! over HTTP.

mod common;

use std::sync::Arc;

use riff_server::log::chunk_name;
use riff_server::store::{Dir, Memory, Store, StoreError};
use serde_json::{Value, json};

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b#tests";
const REPO: &str = "como-technologies/riff";

/// Calls `op` and returns the reply. The call must succeed.
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

/// Calls `op` and returns the status.
async fn status(base: &str, op: &str, body: Value) -> u16 {
    common::client()
        .post(format!("{base}/v1/{op}"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

/// The URI of each session in a `who` reply, gone sessions too. The idle
/// time can change over a restart.
async fn sessions(base: &str) -> Vec<Value> {
    let who = call(base, "who", json!({ "me": MIKE, "all": true })).await;
    let sessions = who["sessions"].as_array().unwrap();
    sessions.iter().map(|s| s["uri"].clone()).collect()
}

/// Makes a state with a post, a direct message, a read and a claim.
async fn fill(base: &str) {
    call(base, "register", json!({ "me": MIKE })).await;
    call(base, "register", json!({ "me": BRETT })).await;
    // A new riff is paused. Mike, a person, resumes it.
    let mike = "riff://mike@pangolin/como-technologies/riff";
    call(base, "resume", json!({ "me": mike, "riff": true })).await;
    let to_brett = json!([{ "user": "brett" }]);
    let post = json!({ "me": MIKE, "thread": "design", "to": to_brett, "body": "look" });
    call(base, "post", post).await;
    let post = json!({ "me": MIKE, "thread": REPO, "body": "one" });
    call(base, "post", post).await;
    call(base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    let claim = json!({ "me": BRETT, "thread": REPO, "item": "issue-6" });
    call(base, "claim", claim).await;
}

async fn the_same_state_after_a_restart(store: Arc<dyn Store>) {
    let (old, base) = common::start_on(store.clone()).await;
    fill(&base).await;
    let before = sessions(&base).await;
    let threads = call(&base, "threads", json!({ "me": BRETT })).await;
    old.save().await.unwrap();

    let (_new, base) = common::start_on(store).await;
    assert_eq!(sessions(&base).await, before);
    assert_eq!(
        call(&base, "threads", json!({ "me": BRETT })).await["threads"][0]["thread"],
        threads["threads"][0]["thread"]
    );
    let read = call(&base, "read", json!({ "me": BRETT, "thread": "design" })).await;
    assert_eq!(read["messages"][0]["body"], "look");
    // The read cursors are in memory: a replay reads a message again, but
    // never misses one.
    let read = call(&base, "read", json!({ "me": BRETT, "thread": REPO })).await;
    assert_eq!(read["messages"][0]["body"], "one");
    // The riff still runs, and Brett still holds the claim.
    let state = call(&base, "riff", json!({ "me": MIKE })).await;
    assert_eq!(state["state"], "running");
    let claim = json!({ "me": MIKE, "thread": REPO, "item": "issue-6" });
    let held = common::held(&base, claim).await;
    assert!(held.starts_with("brett@"), "{held}");
}

#[tokio::test]
async fn a_new_server_on_the_same_store_has_the_same_state() {
    the_same_state_after_a_restart(Arc::new(Memory::default())).await;
}

#[tokio::test]
async fn a_new_server_on_the_same_directory_has_the_same_state() {
    let dir = tempfile::tempdir().unwrap();
    the_same_state_after_a_restart(Arc::new(Dir::new(dir.path()))).await;
    assert!(dir.path().join(chunk_name(1)).exists());
}

#[tokio::test]
async fn a_chunk_of_another_instance_stops_the_server() {
    let store = Memory::default();
    let (server, base) = common::start_on(Arc::new(store.clone())).await;
    call(&base, "register", json!({ "me": MIKE })).await;
    server.save().await.unwrap();
    // Another instance wrote the next chunk.
    let mut records = 0;
    for name in store.list("log/").await.unwrap() {
        let bytes = store.load(&name).await.unwrap().unwrap().bytes;
        records += riff_server::log::decode(&bytes).unwrap().1.len();
    }
    let next = chunk_name(u64::try_from(records).unwrap() + 1);
    store.save(&next, b"other".to_vec(), None).await.unwrap();

    let post = json!({ "me": MIKE, "thread": REPO, "body": "lost" });
    assert_eq!(status(&base, "post", post).await, 503);
    // The server stopped for good (R141): 503, and no more writes.
    server.stopped().await;
    assert_eq!(status(&base, "who", json!({ "me": MIKE })).await, 503);
    let chunks = store.list("log/").await.unwrap().len();
    let post = json!({ "me": MIKE, "thread": REPO, "body": "also lost" });
    let _ = status(&base, "post", post).await;
    assert!(server.save().await.is_ok());
    assert_eq!(store.list("log/").await.unwrap().len(), chunks);
}

#[tokio::test]
async fn a_chunk_that_does_not_read_stops_the_load_and_names_the_chunk() {
    let store = Memory::default();
    let name = chunk_name(1);
    store.save(&name, b"[]".to_vec(), None).await.unwrap();
    let error = common::load_on(Arc::new(store)).await.err().unwrap();
    let StoreError::NotValid { object, fix, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(object, &name);
    assert_eq!(fix, &None);
    assert!(
        error.to_string().ends_with("remove the old state."),
        "{error}"
    );
}

#[tokio::test]
async fn a_gap_in_the_log_stops_the_load() {
    let store = Memory::default();
    let (server, base) = common::start_on(Arc::new(store.clone())).await;
    call(&base, "register", json!({ "me": MIKE })).await;
    call(&base, "register", json!({ "me": BRETT })).await;
    server.save().await.unwrap();
    drop(server);
    let names = store.list("log/").await.unwrap();
    assert!(names.len() >= 2, "{names:?}");
    let gap = Memory::default();
    for name in names.iter().skip(1) {
        let bytes = store.load(name).await.unwrap().unwrap().bytes;
        gap.save(name, bytes, None).await.unwrap();
    }
    let error = common::load_on(Arc::new(gap)).await.err().unwrap();
    assert!(error.to_string().contains("the log needs 1"), "{error}");
    assert!(error.to_string().contains(&names[1]), "{error}");
}

/// The session of `who` in the `who` reply that Mike gets, gone
/// sessions too.
async fn session_of(base: &str, who: &str) -> Value {
    let reply = call(base, "who", json!({ "me": MIKE, "all": true })).await;
    reply["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["uri"].as_str().unwrap().contains(who))
        .unwrap()
        .clone()
}

/// A status stays after a restart: the stop writes a last checkpoint,
/// and the checkpoint keeps the status. A new start of `riff-server`
/// makes it stale (01M4263ZZVY8QJ2METTEVR1W26,
/// 01M4264028A3KVDK10PPERHM0C).
#[tokio::test]
async fn a_status_stays_after_a_restart() {
    let store: Arc<dyn Store> = Arc::new(Memory::default());
    let (old, base) = common::start_on(store.clone()).await;
    fill(&base).await;
    let step = json!({ "step": "write the tests" });
    call(&base, "status", json!({ "me": BRETT, "status": step })).await;
    let before = session_of(&base, "brett").await;
    assert_eq!(before["status"]["step"], "write the tests");
    old.shutdown().await.unwrap();

    let (_new, base) = common::start_on(store).await;
    let brett = session_of(&base, "brett").await;
    assert_eq!(brett["status"]["step"], "write the tests", "{brett}");
    assert_eq!(brett["status"]["stale"], true, "{brett}");
}
