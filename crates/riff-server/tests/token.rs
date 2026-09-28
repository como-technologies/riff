mod common;

use std::sync::Arc;
use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{TokenError, TokenReply};
use riff_server::store::{Loaded, Memory, Store, StoreError, TOKENS, Version};

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
}

#[tokio::test]
async fn tokens_stay_valid_across_a_restart() {
    let store = Memory::default();
    let (service, url) = common::start_on(Arc::new(store.clone())).await;
    let key = Key::generate();
    let jkt = key.thumbprint();
    let first = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let form = |t: &str| format!("grant_type=refresh_token&refresh_token={t}");
    let reply = common::refresh(&url, &key, &form(&first.refresh_token)).await;
    let second: TokenReply = reply.json().await.unwrap();
    // The reply came after the save (R128): the store holds the tokens now.
    assert!(store.load(TOKENS).await.unwrap().is_some());
    drop(service);

    let (restarted, url) = common::start_on(Arc::new(store.clone())).await;
    assert_eq!(
        restarted
            .tokens()
            .check(&second.access_token, &jkt, Instant::now()),
        Ok("mike".to_owned())
    );
    let reply = common::refresh(&url, &key, &form(&second.refresh_token)).await;
    assert_eq!(reply.status(), 200);
    let third: TokenReply = reply.json().await.unwrap();

    // The refresh token that was used before the restart still revokes.
    let reused = common::refresh(&url, &key, &form(&first.refresh_token)).await;
    assert_eq!(error(reused).await, (400, "invalid_grant".into()));
    let (restarted, _) = common::start_on(Arc::new(store)).await;
    assert!(
        restarted
            .tokens()
            .check(&third.access_token, &jkt, Instant::now())
            .is_err(),
        "the revoke was saved too"
    );
}

#[tokio::test]
async fn a_token_change_that_is_not_saved_gets_503() {
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
    let form = format!(
        "grant_type=refresh_token&refresh_token={}",
        first.refresh_token
    );
    let reply = common::refresh(&url, &key, &form).await;
    assert_eq!(error(reply).await, (503, "temporarily_unavailable".into()));
    // A refusal that changes nothing needs no save.
    let unknown = common::refresh(&url, &key, "grant_type=refresh_token&refresh_token=x").await;
    assert_eq!(error(unknown).await, (400, "invalid_grant".into()));
}

/// A store whose first save of the tokens fails.
#[derive(Default)]
struct FailsOnce {
    store: Memory,
    failed: std::sync::atomic::AtomicBool,
}

impl riff_server::store::Store for FailsOnce {
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
        if name == TOKENS && !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Box::pin(async { Err(StoreError::Failed("the disk is busy".into())) });
        }
        self.store.save(name, bytes, known)
    }
}

/// A 503 after a failed save loses the reply of a refresh. The same
/// refresh token again gets a new pair, and the sign-in stays
/// (01M3MX4TG7PNNETZ986DQS10JJ).
#[tokio::test]
async fn a_refresh_again_after_a_503_keeps_the_sign_in() {
    let (service, url) = common::start_on(Arc::new(FailsOnce::default())).await;
    let key = Key::generate();
    let jkt = key.thumbprint();
    let first = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let form = format!(
        "grant_type=refresh_token&refresh_token={}",
        first.refresh_token
    );
    let lost = common::refresh(&url, &key, &form).await;
    assert_eq!(error(lost).await, (503, "temporarily_unavailable".into()));
    let reply = common::refresh(&url, &key, &form).await;
    assert_eq!(reply.status(), 200);
    let second: TokenReply = reply.json().await.unwrap();
    assert_eq!(
        service
            .tokens()
            .check(&second.access_token, &jkt, Instant::now()),
        Ok("mike".to_owned())
    );
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
