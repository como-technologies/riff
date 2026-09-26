mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{TokenError, TokenReply};

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
