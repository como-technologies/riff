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
        .sign_in("mike", &key.thumbprint(), Instant::now())
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

    // The same refresh token again: refused, and the sign-in ends.
    let refused = error(common::refresh(&url, &key, &form).await).await;
    assert_eq!(refused, (400, "invalid_grant".into()));
    assert!(
        service
            .tokens()
            .check(&second.access_token, &jkt, now)
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

/// A store whose saves fail.
struct Broken;

impl riff_server::store::Store for Broken {
    fn load<'a>(
        &'a self,
        _: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Option<Loaded>, StoreError>> {
        Box::pin(async { Ok(None) })
    }

    fn list<'a>(
        &'a self,
        _: &'a str,
    ) -> futures::future::BoxFuture<'a, Result<Vec<String>, StoreError>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn save<'a>(
        &'a self,
        _: &'a str,
        _: Vec<u8>,
        _: Option<Version>,
    ) -> futures::future::BoxFuture<'a, Result<Version, StoreError>> {
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
        .sign_in("mike", &jkt, Instant::now())
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
    let (service, url) = common::start_on(Arc::new(Broken)).await;
    let key = Key::generate();
    let first = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
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
