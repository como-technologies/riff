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
use riff_server::tools::{Mode, cut, cut_with, verify};
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
    // A riff with sign-in: only there, a person ends a sign-in.
    let (service, base) = common::start_signed_on(Arc::new(Memory::default())).await;
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
    // A riff with sign-in: only there, a person ends a sign-in.
    let (service, base) = common::start(true, &[]).await;
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
    let timing = Timing {
        renew_every: RENEW_EVERY,
        ..common::LEASE
    };
    start_with(store, timing).await
}

/// Starts a server on `store` with the lease times `lease`.
async fn start_with(store: Arc<dyn Store>, lease: Timing) -> (Service, String) {
    common::client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease,
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

/// The position of the last record of the log of `store`.
async fn last_position(store: &Memory) -> u64 {
    verify(store).await.unwrap().last.unwrap()
}

/// The run that found the fault of #363, as a test. An instance holds
/// the lease, does not run for more than the live time, and a cut runs.
/// Then the instance runs again, with the removed records in its
/// memory. It does not serve, it writes no chunk, and it does not write
/// the lease (01M3X5TP9CD4NXGPJEGP2Q4RS0, 01M3X5TPBMF81TDVZ7Q4NVXBQX).
#[tokio::test]
async fn an_instance_that_runs_again_after_a_cut_does_not_serve_and_writes_no_chunk() {
    let store = Arc::new(Flaky::default());
    let timing = Timing {
        renew_every: RENEW_EVERY,
        ends_after: Duration::from_secs(1),
        ..common::LEASE
    };
    let (service, base) = start_with(store.clone(), timing).await;
    assert_eq!(status(&base, "register", json!({ "me": MIKE })).await, 200);
    service.save().await.unwrap();
    assert_eq!(status(&base, "register", json!({ "me": BRETT })).await, 200);
    service.save().await.unwrap();
    let last = last_position(&store.store).await;

    // The instance does not run: it does not reach the store.
    store.down.store(true, Ordering::SeqCst);
    sleep(timing.ends_after + Duration::from_millis(200)).await;

    // The lease ended by its age: the cut takes it, and cuts.
    let removed = cut_with(&store.store, last - 1, Mode::Remove, &timing)
        .await
        .unwrap();
    assert_eq!(removed.records.len(), 1, "{removed:?}");
    assert_eq!(removed.held, None);
    let after_cut = chunks(&store.store).await;
    let lease = lease_json(&store.store).await;
    assert!(lease["id"].as_str().unwrap().starts_with("cut-"), "{lease}");

    // The instance runs again.
    store.down.store(false, Ordering::SeqCst);
    timeout(Duration::from_secs(5), service.stopped())
        .await
        .unwrap();
    assert!(service.lease_ended(), "the instance stops with an error");
    sleep(timing.valid_for).await;
    let third = "riff://ann@heron/como-technologies/riff?session=c";
    assert_eq!(status(&base, "register", json!({ "me": third })).await, 503);
    service.save().await.unwrap();
    assert_eq!(chunks(&store.store).await, after_cut, "no new chunk");
    assert_eq!(
        lease_json(&store.store).await,
        lease,
        "no write of the lease"
    );
    assert_eq!(last_position(&store.store).await, last - 1);
    assert!(verify(&store.store).await.unwrap().problems.is_empty());
}

/// The clock of the tool says that the lease ended, but its instance
/// runs: the take of the lease by the cut stops the instance, before
/// the cut reads the log (01M3X5TP9CD4NXGPJEGP2Q4RS0).
#[tokio::test]
async fn the_take_of_the_lease_by_a_cut_stops_an_instance_that_runs() {
    let store = Memory::default();
    let (service, base) = common::start_on(Arc::new(store.clone())).await;
    assert_eq!(status(&base, "register", json!({ "me": MIKE })).await, 200);
    service.save().await.unwrap();
    assert_eq!(status(&base, "register", json!({ "me": BRETT })).await, 200);
    service.save().await.unwrap();
    let last = last_position(&store).await;

    // The lease of the instance, with a time that is 95 seconds old.
    let lease = store.load(LEASE).await.unwrap().unwrap();
    let mut old: Value = serde_json::from_slice(&lease.bytes).unwrap();
    old["renewed_at_ms"] = json!(now_ms() - 95_000);
    let old = serde_json::to_vec(&old).unwrap();
    store.save(LEASE, old, Some(lease.version)).await.unwrap();

    let removed = cut_with(&store, last - 1, Mode::Remove, &common::LEASE)
        .await
        .unwrap();
    assert_eq!(removed.records.len(), 1, "{removed:?}");
    // The instance read the ID of the cut during the wait of the cut.
    timeout(Duration::from_millis(100), service.stopped())
        .await
        .unwrap();
    assert!(!service.lease_ended());
    let after_cut = chunks(&store).await;
    let third = "riff://ann@heron/como-technologies/riff?session=c";
    assert_eq!(status(&base, "register", json!({ "me": third })).await, 503);
    service.save().await.unwrap();
    assert_eq!(chunks(&store).await, after_cut, "no new chunk");
    assert!(verify(&store).await.unwrap().problems.is_empty());
}

/// A deploy: the old and the new instance run at the same time for a
/// short time. The lease is live all the time, so a cut refuses. The
/// old instance does not write the time over the lease of the new one,
/// and its shutdown does not end that lease.
#[tokio::test]
async fn in_a_deploy_the_lease_stays_live_and_names_the_new_instance() {
    let store = Memory::default();
    let timing = Timing::default();
    let (old, old_base) = start_renewing(Arc::new(store.clone())).await;
    let old_id = lease_json(&store).await["id"].as_str().unwrap().to_owned();
    let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
    assert_eq!(held.id, old_id);

    let (new, new_base) = start_renewing(Arc::new(store.clone())).await;
    timeout(Duration::from_secs(5), old.stopped())
        .await
        .unwrap();
    let new_id = lease_json(&store).await["id"].as_str().unwrap().to_owned();
    assert_ne!(new_id, old_id);
    // The old instance still runs, and writes no time: each write of the
    // lease is from the new instance.
    sleep(RENEW_EVERY * 4).await;
    let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
    assert_eq!(held.id, new_id);
    let who = json!({ "me": "riff://mike@pangolin" });
    assert_eq!(status(&old_base, "who", who.clone()).await, 503);
    assert_eq!(status(&new_base, "who", who).await, 200);

    // The shutdown of the old instance does not end the lease.
    old.shutdown().await.unwrap();
    let held = holder(&store, now_ms(), &timing).await.unwrap().unwrap();
    assert_eq!(held.id, new_id);
    let refused = cut(&store, 0, Mode::Remove).await.unwrap_err();
    assert!(
        refused
            .to_string()
            .contains(&format!("the server instance {new_id} holds the lease")),
        "{refused}"
    );

    new.shutdown().await.unwrap();
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
