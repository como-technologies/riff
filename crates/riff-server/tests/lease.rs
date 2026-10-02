//! Only one instance serves at a time (R29, R137-R142), over HTTP.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::future::{BoxFuture, FutureExt};
use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::lease::{Timing, holder, now_ms};
use riff_server::store::{LEASE, Loaded, Memory, Store, StoreError, Version};
use serde_json::{Value, json};
use tokio::time::{sleep, timeout};

const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a#api";
const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=b#tests";

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

async fn chunks(store: &Memory) -> Vec<String> {
    store.list("log/").await.unwrap()
}

#[tokio::test]
async fn a_new_server_stops_the_old_one() {
    let store = Memory::default();
    let (old, old_base) = common::start_on(Arc::new(store.clone())).await;
    assert_eq!(
        status(&old_base, "register", json!({ "me": MIKE })).await,
        200
    );
    let watch = common::client()
        .get(format!("{old_base}/v1/watch"))
        .query(&[("uri", MIKE)])
        .send()
        .await
        .unwrap();
    assert_eq!(watch.status(), 200);
    old.save().await.unwrap();
    let saved = chunks(&store).await;

    let (new, new_base) = common::start_on(Arc::new(store.clone())).await;
    timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    assert_eq!(
        status(&old_base, "who", json!({ "me": "riff://mike@pangolin" })).await,
        503
    );
    // The watch stream of the old server ends.
    timeout(Duration::from_secs(5), watch.bytes())
        .await
        .unwrap()
        .unwrap();
    // The end of the stream changed the old state, but only the new
    // server saves.
    old.save().await.unwrap();
    assert_eq!(chunks(&store).await, saved);
    assert_eq!(
        status(&new_base, "register", json!({ "me": BRETT })).await,
        200
    );
    new.save().await.unwrap();
    assert_ne!(chunks(&store).await, saved);
}

#[tokio::test]
async fn a_new_server_refuses_a_proof_from_before_it_served() {
    let before = common::now();
    let (service, base) = common::start_on(Arc::new(Memory::default())).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let url = format!("{base}/v1/revoke");
    let revoke = |iat: u64| {
        common::client()
            .post(&url)
            .header(
                "dpop",
                key.proof("POST", &url, Some(&pair.access_token), iat),
            )
            .header("authorization", format!("DPoP {}", pair.access_token))
            .json(&json!({}))
            .send()
    };
    assert_eq!(revoke(before).await.unwrap().status(), 401);
    assert_eq!(revoke(common::now()).await.unwrap().status(), 200);
}

#[tokio::test]
async fn a_server_with_no_store_refuses_a_proof_from_before_it_started() {
    let before = common::now() - 1;
    let (service, base) = common::start(false, &[]).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let url = format!("{base}/v1/revoke");
    let reply = common::client()
        .post(&url)
        .header(
            "dpop",
            key.proof("POST", &url, Some(&pair.access_token), before),
        )
        .header("authorization", format!("DPoP {}", pair.access_token))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

/// A memory store that fails each call while it is down, and each save
/// of the lease while `no_lease_save` is true.
#[derive(Default)]
struct Flaky {
    store: Memory,
    down: AtomicBool,
    no_lease_save: AtomicBool,
}

impl Flaky {
    fn check(&self) -> Result<(), StoreError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(StoreError::Failed("the store is down".into()));
        }
        Ok(())
    }
}

impl Store for Flaky {
    fn load<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        match self.check() {
            Ok(()) => self.store.load(name),
            Err(e) => futures::future::ready(Err(e)).boxed(),
        }
    }

    fn list<'a>(&'a self, prefix: &'a str) -> BoxFuture<'a, Result<Vec<String>, StoreError>> {
        match self.check() {
            Ok(()) => self.store.list(prefix),
            Err(e) => futures::future::ready(Err(e)).boxed(),
        }
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> BoxFuture<'a, Result<Version, StoreError>> {
        if name == LEASE && self.no_lease_save.load(Ordering::SeqCst) {
            let error = StoreError::Failed("the lease is not saved".into());
            return futures::future::ready(Err(error)).boxed();
        }
        match self.check() {
            Ok(()) => self.store.save(name, bytes, known),
            Err(e) => futures::future::ready(Err(e)).boxed(),
        }
    }

    fn delete<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        match self.check() {
            Ok(()) => self.store.delete(name),
            Err(e) => futures::future::ready(Err(e)).boxed(),
        }
    }
}

#[tokio::test]
async fn a_server_that_cannot_read_the_lease_replies_503_until_it_can() {
    let store = Arc::new(Flaky::default());
    let (_service, base) = common::start_on(store.clone()).await;
    assert_eq!(
        status(&base, "who", json!({ "me": "riff://mike@pangolin" })).await,
        200
    );

    store.down.store(true, Ordering::SeqCst);
    sleep(common::LEASE.valid_for + common::LEASE.read_every * 2).await;
    assert_eq!(
        status(&base, "who", json!({ "me": "riff://mike@pangolin" })).await,
        503
    );

    store.down.store(false, Ordering::SeqCst);
    sleep(common::LEASE.read_every * 4).await;
    assert_eq!(
        status(&base, "who", json!({ "me": "riff://mike@pangolin" })).await,
        200
    );
}

/// The time between two writes of the time to the lease, in the tests
/// of the renewal.
const RENEW_EVERY: Duration = Duration::from_millis(100);

/// Starts a server on `store` that writes the time to the lease each
/// [`RENEW_EVERY`].
async fn start_renewing(store: Arc<dyn Store>) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease: Timing {
            renew_every: RENEW_EVERY,
            ..common::LEASE
        },
        save_every: common::SAVE_EVERY,
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// The lease object of `store` as JSON.
async fn lease_json(store: &Memory) -> Value {
    let lease = store.load(LEASE).await.unwrap().unwrap();
    serde_json::from_slice(&lease.bytes).unwrap()
}

/// 01M3X34282SG0DJ6X34F90HS26, 01M3X342ARX5Y7R9ZJDT12R9A1: an instance
/// writes the time to its lease, and ends the lease at its shutdown.
#[tokio::test]
async fn a_server_writes_the_time_to_the_lease_and_ends_it_at_its_shutdown() {
    let store = Memory::default();
    let (service, _base) = start_renewing(Arc::new(store.clone())).await;
    let first = lease_json(&store).await;
    sleep(RENEW_EVERY * 4).await;
    let second = lease_json(&store).await;
    assert_eq!(first["id"], second["id"]);
    assert!(
        second["renewed_at_ms"].as_u64() > first["renewed_at_ms"].as_u64(),
        "{first} {second}"
    );
    let timing = Timing::default();
    let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
    assert_eq!(held.id, first["id"].as_str().unwrap());

    service.shutdown().await.unwrap();
    assert_eq!(lease_json(&store).await["ended"], true);
    assert_eq!(holder(&store, now_ms(), &timing).await.unwrap(), None);
    // The lease stays ended: the instance writes it no more.
    sleep(RENEW_EVERY * 4).await;
    assert_eq!(holder(&store, now_ms(), &timing).await.unwrap(), None);
}

/// 01M3X34282SG0DJ6X34F90HS26: while a renewal that is due fails, the
/// instance does not serve. So an instance never writes the log long
/// after the time in its lease.
#[tokio::test]
async fn a_server_that_cannot_renew_the_lease_replies_503_until_it_can() {
    let store = Arc::new(Flaky::default());
    let (_service, base) = start_renewing(store.clone()).await;
    let who = json!({ "me": "riff://mike@pangolin" });
    assert_eq!(status(&base, "who", who.clone()).await, 200);

    // Each read of the lease still works.
    store.no_lease_save.store(true, Ordering::SeqCst);
    sleep(common::LEASE.valid_for + RENEW_EVERY * 3).await;
    assert_eq!(status(&base, "who", who.clone()).await, 503);

    store.no_lease_save.store(false, Ordering::SeqCst);
    sleep(common::LEASE.read_every * 4).await;
    assert_eq!(status(&base, "who", who).await, 200);
}
