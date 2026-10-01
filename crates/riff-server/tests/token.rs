mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use riff_core::dpop::Key;
use riff_core::wire::{TokenError, TokenReply};
use riff_server::store::{Loaded, Memory, SIGN_INS, Store, StoreError, Version};

async fn error(reply: reqwest::Response) -> (u16, String) {
    assert_eq!(reply.headers()["cache-control"], "no-store");
    let status = reply.status().as_u16();
    (status, reply.json::<TokenError>().await.unwrap().error)
}

#[tokio::test]
async fn refresh_rotates_and_reuse_revokes() {
    let (service, url) = common::start(false, &[]).await;
    let key = Key::generate();
    let first = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let form = format!(
        "grant_type=refresh_token&refresh_token={}",
        first.refresh_token
    );

    let reply = common::refresh(&url, &key, &form).await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.headers()["cache-control"], "no-store");
    let second: TokenReply = reply.json().await.unwrap();
    assert_eq!(second.token_type, "DPoP");
    assert!(second.expires_in <= 600);
    let (now, jkt) = (Instant::now(), key.thumbprint());
    assert_eq!(
        service.tokens().check(&second.access_token, &jkt, now),
        Ok("mike".to_owned())
    );

    // After the next refresh, the first refresh token again: refused,
    // and the sign-in ends.
    let next = format!(
        "grant_type=refresh_token&refresh_token={}",
        second.refresh_token
    );
    let third: TokenReply = common::refresh(&url, &key, &next)
        .await
        .json()
        .await
        .unwrap();
    let refused = error(common::refresh(&url, &key, &form).await).await;
    assert_eq!(refused, (400, "invalid_grant".into()));
    assert!(
        service
            .tokens()
            .check(&third.access_token, &jkt, Instant::now())
            .is_err()
    );
}

#[tokio::test]
async fn unknown_tokens_and_grants_are_refused() {
    let (_, url) = common::start(false, &[]).await;
    let key = Key::generate();
    let unknown = common::refresh(&url, &key, "grant_type=refresh_token&refresh_token=nope").await;
    assert_eq!(error(unknown).await, (400, "invalid_grant".into()));
    let password = common::refresh(&url, &key, "grant_type=password&refresh_token=nope").await;
    assert_eq!(
        error(password).await,
        (400, "unsupported_grant_type".into())
    );
}

/// A store whose saves fail, except the saves of the lease.
#[derive(Default)]
struct Broken(Memory);

fn form(token: &str) -> String {
    format!("grant_type=refresh_token&refresh_token={token}")
}

async fn pair(reply: reqwest::Response) -> TokenReply {
    let status = reply.status();
    let text = reply.text().await.unwrap();
    assert_eq!(status, 200, "{text}");
    serde_json::from_str(&text).unwrap()
}

impl riff_server::store::Store for Broken {
    fn load<'a>(
        &'a self,
        name: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.0.load(name)
    }

    fn list<'a>(
        &'a self,
        prefix: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.0.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> futures::future::BoxFuture<'a, Result<Version, StoreError>> {
        if name == riff_server::store::LEASE {
            return self.0.save(name, bytes, known);
        }
        Box::pin(async { Err(StoreError::Failed("the disk is gone".into())) })
    }

    fn delete<'a>(
        &'a self,
        _name: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<(), StoreError>> {
        Box::pin(async { Err(StoreError::Failed("the disk is gone".into())) })
    }
}

/// A refresh token works after a restart. An access token is only in
/// memory: after a restart, the client refreshes one time
/// (01M3TFG551C76BP4TRA32P7VC3).
#[tokio::test]
async fn refresh_tokens_stay_valid_across_a_restart() {
    let store = Memory::default();
    let (service, url) = common::start_on(Arc::new(store.clone())).await;
    let key = Key::generate();
    let jkt = key.thumbprint();
    let first = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let second = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    let third = pair(common::refresh(&url, &key, &form(&second.refresh_token)).await).await;
    // A stop saves the token store (R129).
    service.shutdown().await.unwrap();
    assert!(store.load(SIGN_INS).await.unwrap().is_some());
    drop(service);

    let (restarted, url) = common::start_on(Arc::new(store.clone())).await;
    assert!(
        restarted
            .tokens()
            .check(&third.access_token, &jkt, Instant::now())
            .is_err(),
        "an access token is only in memory"
    );
    let fourth = pair(common::refresh(&url, &key, &form(&third.refresh_token)).await).await;
    assert_eq!(
        restarted
            .tokens()
            .check(&fourth.access_token, &jkt, Instant::now()),
        Ok("mike".to_owned())
    );

    // A generation from before the restart still ends the sign-in.
    let reused = common::refresh(&url, &key, &form(&first.refresh_token)).await;
    assert_eq!(error(reused).await, (400, "invalid_grant".into()));
    restarted.shutdown().await.unwrap();
    let (restarted, url) = common::start_on(Arc::new(store)).await;
    assert_eq!(restarted.tokens().chains(), 0, "the revoke was saved too");
    let gone = common::refresh(&url, &key, &form(&fourth.refresh_token)).await;
    assert_eq!(error(gone).await, (400, "invalid_grant".into()));
}

/// A store whose saves of the token store fail while `failing` is true.
/// It counts the saves of the token store, and keeps the time of each.
#[derive(Clone, Default)]
struct Failing {
    store: Memory,
    failing: Arc<AtomicBool>,
    saves: Arc<std::sync::Mutex<Vec<Instant>>>,
}

impl Failing {
    fn busy(&self, on: bool) {
        self.failing.store(on, Ordering::SeqCst);
    }

    fn saves(&self) -> Vec<Instant> {
        self.saves.lock().unwrap().clone()
    }

    /// Waits until the store got `count` saves of the token store.
    async fn saved(&self, count: usize) {
        for _ in 0..500 {
            if self.saves().len() >= count {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the store got {} saves, not {count}", self.saves().len());
    }
}

impl riff_server::store::Store for Failing {
    fn load<'a>(
        &'a self,
        name: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        self.store.load(name)
    }

    fn list<'a>(
        &'a self,
        prefix: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Vec<String>, StoreError>> {
        self.store.list(prefix)
    }

    fn save<'a>(
        &'a self,
        name: &'a str,
        bytes: Vec<u8>,
        known: Option<Version>,
    ) -> futures::future::BoxFuture<'a, Result<Version, StoreError>> {
        if name == SIGN_INS {
            self.saves.lock().unwrap().push(Instant::now());
            if self.failing.load(Ordering::SeqCst) {
                return Box::pin(async { Err(StoreError::Failed("the disk is busy".into())) });
            }
        }
        self.store.save(name, bytes, known)
    }

    fn delete<'a>(
        &'a self,
        name: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<(), StoreError>> {
        self.store.delete(name)
    }
}

/// A sign-in on a store, and the server of it.
async fn signed_in_on(store: &Failing) -> (riff_server::Service, String, Key, TokenReply) {
    let (service, url) = common::start_on(Arc::new(store.clone())).await;
    let key = Key::generate();
    let first = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    (service, url, key, first)
}

/// A refresh gets its reply before the write of the token store
/// (01M3TFG527M04TA7ESM970X3B8). The server then stops with no write: a
/// crash. The saved store is one generation behind, and the sign-in
/// stays (01M3TFG4WE7CZQ4TCJE2NTC52E).
#[tokio::test]
async fn a_crash_after_a_refresh_and_before_the_write_does_not_end_the_sign_in() {
    let store = Failing::default();
    let (service, url, key, first) = signed_in_on(&store).await;
    let second = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    // The store holds the second generation.
    service.save().await.unwrap();
    let before = store.store.load(SIGN_INS).await.unwrap().unwrap();

    // No write works from now on. The refresh still gets its pair.
    store.busy(true);
    let third = pair(common::refresh(&url, &key, &form(&second.refresh_token)).await).await;
    assert!(
        third.refresh_token.contains(".3."),
        "{}",
        third.refresh_token
    );
    // The crash: the old server writes nothing more. A new server takes
    // the lease, and loads the old object.
    let (restarted, url) = common::start_on(Arc::new(store.clone())).await;
    service.stopped().await;
    let after = store.store.load(SIGN_INS).await.unwrap().unwrap();
    assert_eq!(after.bytes, before.bytes);
    store.busy(false);
    let fourth = pair(common::refresh(&url, &key, &form(&third.refresh_token)).await).await;
    assert!(
        fourth.refresh_token.contains(".4."),
        "{}",
        fourth.refresh_token
    );
    assert_eq!(
        restarted
            .tokens()
            .check(&fourth.access_token, &key.thumbprint(), Instant::now()),
        Ok("mike".to_owned())
    );
    // The token of the saved generation is now old, and the chain goes on.
    assert!(
        common::refresh(&url, &key, &form(&fourth.refresh_token))
            .await
            .status()
            .is_success()
    );
}

/// After a restart, the current generation of the saved store is good
/// too: the client that did not refresh before the crash goes on.
#[tokio::test]
async fn after_a_restart_the_saved_generation_is_good() {
    let store = Failing::default();
    let (service, url, key, first) = signed_in_on(&store).await;
    let second = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    service.save().await.unwrap();
    drop(service);
    let (_restarted, url) = common::start_on(Arc::new(store.clone())).await;
    let third = pair(common::refresh(&url, &key, &form(&second.refresh_token)).await).await;
    assert!(
        third.refresh_token.contains(".3."),
        "{}",
        third.refresh_token
    );
}

/// While the last write of the token store failed, a refresh writes it
/// again first, and gets 503 when that write fails too
/// (01M3TFG527M04TA7ESM970X3B8). A 503 loses no generation: the same
/// refresh token works after the store is back.
#[tokio::test]
async fn a_refresh_gets_503_while_the_token_store_is_not_written() {
    let store = Failing::default();
    let (service, url, key, first) = signed_in_on(&store).await;
    service.save().await.unwrap();
    store.busy(true);
    // The reply does not wait for the write, so this refresh works.
    let second = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    // Two more saves: the first one failed, and the server knows it.
    let saves = store.saves().len();
    store.saved(saves + 2).await;
    // A refusal that changes nothing needs no write.
    let unknown = common::refresh(&url, &key, &form("x")).await;
    assert_eq!(error(unknown).await, (400, "invalid_grant".into()));
    // The write failed. The next refresh does not go on.
    for _ in 0..2 {
        let refused = common::refresh(&url, &key, &form(&second.refresh_token)).await;
        assert_eq!(
            error(refused).await,
            (503, "temporarily_unavailable".into())
        );
    }
    store.busy(false);
    let third = pair(common::refresh(&url, &key, &form(&second.refresh_token)).await).await;
    assert_eq!(
        service
            .tokens()
            .check(&third.access_token, &key.thumbprint(), Instant::now()),
        Ok("mike".to_owned())
    );
}

/// A sign-in gets its reply only after the write of the token store
/// (R128). On a store whose saves fail, a refresh gets 503 (R150).
#[tokio::test]
async fn a_refresh_on_a_store_that_never_saves_gets_503() {
    let (service, url) = common::start_on(Arc::new(Broken::default())).await;
    let key = Key::generate();
    let first = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap();
    let reply = common::refresh(&url, &key, &form(&first.refresh_token)).await;
    assert_eq!(error(reply).await, (503, "temporarily_unavailable".into()));
    // A refusal that changes nothing needs no save.
    let unknown = common::refresh(&url, &key, &form("x")).await;
    assert_eq!(error(unknown).await, (400, "invalid_grant".into()));
}

/// The same refresh token two times, before the next refresh: the reply
/// of the first use was lost. The sign-in stays
/// (01M3MX4TG7PNNETZ986DQS10JJ).
#[tokio::test]
async fn a_refresh_again_after_a_lost_reply_keeps_the_sign_in() {
    let store = Failing::default();
    let (service, url, key, first) = signed_in_on(&store).await;
    let jkt = key.thumbprint();
    let lost = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    let second = pair(common::refresh(&url, &key, &form(&first.refresh_token)).await).await;
    let check = |token: &str| service.tokens().check(token, &jkt, Instant::now());
    assert_eq!(check(&second.access_token), Ok("mike".to_owned()));
    assert!(check(&lost.access_token).is_err());

    // A lost reply at a stop: the server saved the new generation, and
    // the client has the old one. The sign-in stays after the restart.
    let third = pair(common::refresh(&url, &key, &form(&second.refresh_token)).await).await;
    service.shutdown().await.unwrap();
    drop((service, third));
    let (_restarted, url) = common::start_on(Arc::new(store.clone())).await;
    let again = common::refresh(&url, &key, &form(&second.refresh_token)).await;
    assert_eq!(again.status(), 200);
}

/// The server writes the token store at most one time each second
/// (R127), also when many changes come. The test counts the writes in
/// the time of the test, so a slow machine changes no result.
#[tokio::test]
async fn the_token_store_gets_at_most_one_write_each_second() {
    let store = Failing::default();
    let every = Duration::from_secs(1);
    let test_started = Instant::now();
    let (service, url) = common::start_on_every(Arc::new(store.clone()), every).await;
    let key = Key::generate();
    let mut token = service
        .tokens()
        .sign_in(
            "mike@comotechnologies.io",
            &key.thumbprint(),
            Instant::now(),
        )
        .unwrap()
        .refresh_token;
    // Changes for more than two seconds: refreshes that do not wait, and
    // saves that wait for their write.
    let started = Instant::now();
    let mut changes = 0;
    while started.elapsed() < Duration::from_millis(2200) {
        token = pair(common::refresh(&url, &key, &form(&token)).await)
            .await
            .refresh_token;
        changes += 1;
        if changes % 20 == 0 {
            service.save().await.unwrap();
        }
    }
    service.save().await.unwrap();
    let saves = store.saves();
    let lasted = test_started.elapsed();
    assert!(
        changes > saves.len(),
        "{changes} changes, {} saves",
        saves.len()
    );
    assert!(saves.len() >= 3, "{} saves", saves.len());
    // Each write is one `every` or more after the write before it, and
    // each write is in the time of the test. So the time of the test
    // holds one `every` for each write after the first.
    let most = 1 + (lasted.as_millis() / every.as_millis()) as usize;
    assert!(
        saves.len() <= most,
        "{} saves in {lasted:?}, at most {most}",
        saves.len()
    );
    // The last write holds the last change.
    drop(service);
    let (_restarted, url) = common::start_on(Arc::new(store.clone())).await;
    pair(common::refresh(&url, &key, &form(&token)).await).await;
}

/// `riff login` and a refresh work with each version of `riff`: the
/// server checks no version on `/v1/token` and `/v1/sign-in`
/// (01M3MX4V43SF2XFCZWANHD19WV). A call with no version header shows it.
#[tokio::test]
async fn the_token_and_the_sign_in_take_each_version() {
    let (_service, url) = common::start(false, &[]).await;
    let client = reqwest::Client::new();
    let token = client
        .post(format!("{url}/v1/token"))
        .form(&[("grant_type", "refresh_token"), ("refresh_token", "nope")])
        .send()
        .await
        .unwrap();
    assert_eq!(error(token).await, (400, "invalid_dpop_proof".into()));
    let sign_in = client
        .get(format!("{url}/v1/sign-in"))
        .send()
        .await
        .unwrap();
    assert_ne!(sign_in.status(), 409);
    // Each other call still checks the version.
    let join = client
        .post(format!("{url}/v1/join"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(join.status(), 409);
}
