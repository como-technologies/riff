//! The start order: load, then the port, then the lease
//! (01M52JYGHHK5X0XS39VFT3ZGGM, 01M52JYGM7WH565B7PCVBEJC17).

use crate::common;

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::future::{BoxFuture, FutureExt};
use riff_state::log::chunk_name;
use riff_state::store::{LEASE, Loaded, Memory, Store, StoreError, Version};
use serde_json::{Value, json};
use tokio::sync::Notify;
use tokio::time::{sleep, timeout};

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b#tests";
const REPO: &str = "como-technologies/riff";

/// A chunk of a later format: this build cannot load it. Its position
/// is far after the log, so the old instance never writes its name.
const LATER_CHUNK: u64 = 999;
const LATER_FORMAT: &str = "{\"format\":2,\"first\":999}\n";

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

/// The URI of each session in a `who` reply, gone sessions too.
async fn sessions(base: &str) -> Vec<String> {
    let who = call(base, "who", json!({ "me": MIKE, "all": true })).await;
    let sessions = who["sessions"].as_array().unwrap();
    sessions
        .iter()
        .map(|s| s["uri"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_build_that_cannot_load_takes_no_lease_and_the_old_instance_serves_on() {
    let store = Memory::default();
    let (old, base) = common::start_on(Arc::new(store.clone())).await;
    call(&base, "register", json!({ "me": MIKE })).await;
    old.save().await.unwrap();
    let lease = store.load(LEASE).await.unwrap().unwrap();

    let later = chunk_name(LATER_CHUNK);
    let bytes = LATER_FORMAT.as_bytes().to_vec();
    store.save(&later, bytes, None).await.unwrap();
    let error = common::load_on(Arc::new(store.clone()))
        .await
        .err()
        .unwrap();
    let StoreError::NotValid { object, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(object, &later);

    // The lease still names the old instance.
    let now = store.load(LEASE).await.unwrap().unwrap();
    assert_eq!((now.bytes, now.version), (lease.bytes, lease.version));
    // The old instance reads the lease some times, and serves on. It
    // also writes.
    sleep(common::LEASE.valid_for + common::LEASE.read_every * 4).await;
    assert!(
        timeout(Duration::from_millis(1), old.stopped())
            .await
            .is_err()
    );
    call(&base, "register", json!({ "me": BRETT })).await;
    assert_eq!(sessions(&base).await.len(), 2);
}

/// A memory store that holds the first write of the lease, once armed,
/// until the test lets it go.
#[derive(Default)]
struct AtLease {
    store: Memory,
    armed: AtomicBool,
    /// The write of the lease waits now: the load is done.
    reached: Notify,
    go: Notify,
}

impl Store for AtLease {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.store.load(name)
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.store.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        if name == LEASE && self.armed.swap(false, Ordering::SeqCst) {
            return async move {
                self.reached.notify_one();
                self.go.notified().await;
                self.store.save(name, bytes, known).await
            }
            .boxed();
        }
        self.store.save(name, bytes, known)
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        self.store.delete(name)
    }
}

#[tokio::test]
async fn a_good_start_applies_the_chunks_written_between_the_load_and_the_lease() {
    let store = Arc::new(AtLease::default());
    let (old, old_base) = common::start_on(store.clone()).await;
    call(&old_base, "register", json!({ "me": MIKE })).await;
    // A new riff is paused. Mike, a person, resumes it.
    let mike = "riff://mike@pangolin/como-technologies/riff";
    call(&old_base, "resume", json!({ "me": mike, "riff": true })).await;
    old.save().await.unwrap();
    let chunks = store.list("log/").await.unwrap().len();

    store.armed.store(true, Ordering::SeqCst);
    let new = tokio::spawn(common::start_on(store.clone()));
    isolated::in_time(Duration::from_secs(5), store.reached.notified())
        .await
        .expect("the new instance takes the lease after its load");

    // The new instance loaded. The old one still serves, and writes.
    call(&old_base, "register", json!({ "me": BRETT })).await;
    let claim = json!({ "me": BRETT, "thread": REPO, "item": "issue-339" });
    let reply = call(&old_base, "claim", claim).await;
    assert!(
        reply["holder"]
            .as_str()
            .unwrap()
            .contains("claim=issue-339")
    );
    assert!(store.list("log/").await.unwrap().len() > chunks);

    store.go.notify_one();
    let (_new, new_base) = isolated::in_time(Duration::from_secs(5), new)
        .await
        .unwrap()
        .unwrap();
    isolated::in_time(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    let uris = sessions(&new_base).await;
    assert!(
        uris.iter()
            .any(|uri| uri.contains("session=b") && uri.contains("claim=issue-339")),
        "{uris:?}"
    );
    // The claim holds for the grace period: no other session gets it.
    let claim = json!({ "me": MIKE, "thread": REPO, "item": "issue-339" });
    let held = common::held(&new_base, claim).await;
    assert!(held.contains("holds issue-339"), "{held}");
}

async fn open(listen: &str) -> bool {
    tokio::net::TcpStream::connect(listen).await.is_ok()
}

/// The real `riff-server` with the real lease wait of 15 seconds. Its
/// port opens after the load and before the lease, and each call gets
/// 503 until it serves (01M52JYGHHK5X0XS39VFT3ZGGM,
/// 01M52JYGM7WH565B7PCVBEJC17).
#[tokio::test]
async fn the_port_opens_after_the_load_and_gives_503_until_the_server_serves() {
    let dir = tempfile::tempdir().unwrap();
    let listen = common::free_address();
    let log = tempfile::NamedTempFile::new().unwrap();
    let _server = common::Server(common::server_on_dir(dir.path(), &listen, &log));

    // The port opens before the lease is there.
    let lease = dir.path().join(LEASE);
    let span = isolated::Span::start();
    loop {
        let leased = lease.exists();
        if open(&listen).await {
            break;
        }
        assert!(!leased, "a lease and no open port");
        assert!(span.within(Duration::from_secs(10)), "no open port");
        sleep(Duration::from_millis(20)).await;
    }
    let opened = isolated::Span::start();

    // During the wait for the lease, each call gets 503 of riff-server.
    let base = format!("http://{listen}");
    let register = || {
        common::client()
            .post(format!("{base}/v1/register"))
            .json(&json!({ "me": MIKE }))
            .send()
    };
    let reply = register().await.unwrap();
    assert_eq!(reply.status(), 503);
    assert!(reply.headers().contains_key("riff-build"), "{reply:?}");
    assert_eq!(reply.text().await.unwrap(), riff_server::listen::STARTING);

    // After the wait, the server serves.
    while register().await.unwrap().status() == 503 {
        assert!(span.within(Duration::from_secs(40)), "it does not serve");
        sleep(Duration::from_millis(100)).await;
    }
    // The real wait is 15 seconds, and the port opened before the take.
    let waited = opened.wall();
    assert!(waited >= Duration::from_secs(14), "it served after {waited:?}");
    call(&base, "register", json!({ "me": MIKE })).await;
    let log = fs::read_to_string(log.path()).unwrap();
    let at = |text: &str| {
        log.find(text)
            .unwrap_or_else(|| panic!("no {text} in {log}"))
    };
    assert!(at("loaded the checkpoint") < at("riff-server listens on"), "{log}");
    assert!(at("riff-server listens on") < at("took the lease"), "{log}");
    assert!(
        at("took the lease") < at("applied the records that came since the load"),
        "{log}"
    );
    assert!(
        at("applied the records that came since the load") < at("riff-server serves"),
        "{log}"
    );
}

#[tokio::test]
async fn a_riff_server_that_cannot_load_exits_with_no_lease_and_no_open_port() {
    let dir = tempfile::tempdir().unwrap();
    let chunk = dir.path().join(chunk_name(LATER_CHUNK));
    fs::create_dir_all(chunk.parent().unwrap()).unwrap();
    fs::write(&chunk, LATER_FORMAT).unwrap();
    let listen = common::free_address();
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut child = common::server_on_dir(dir.path(), &listen, &log);

    // It exits long before the end of a lease wait.
    let span = isolated::Span::start();
    let status = loop {
        assert!(!open(&listen).await, "the port is open");
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if !span.within(Duration::from_secs(10)) {
            let _ = child.kill();
            panic!("riff-server did not exit");
        }
        sleep(Duration::from_millis(20)).await;
    };
    assert!(!status.success());
    assert!(!dir.path().join(LEASE).exists());
    let log = fs::read_to_string(log.path()).unwrap();
    assert!(
        log.contains("riff-server stops: cannot read the saved object"),
        "{log}"
    );
    assert!(log.contains("00000000000000000999.jsonl"), "{log}");
}
