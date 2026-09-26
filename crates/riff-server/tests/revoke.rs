//! `POST /v1/revoke` over HTTP (R20).

mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{Revoke, Revoked};

async fn revoke(base: &str, auth: Option<(&Key, &str)>, user: Option<&str>) -> (u16, String) {
    let url = format!("{base}/v1/revoke");
    let request = match auth {
        Some((key, token)) => common::post(&url, key, Some(token)),
        None => reqwest::Client::new().post(&url),
    };
    let reply = request
        .json(&Revoke {
            user: user.map(str::to_owned),
        })
        .send()
        .await
        .unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

#[tokio::test]
async fn a_person_revokes_each_of_their_sign_ins() {
    let (service, url) = common::start(false, &[]).await;
    let now = Instant::now();
    let (laptop_key, desktop_key, brett_key) = (Key::generate(), Key::generate(), Key::generate());
    let sign_in = |user, key: &Key| {
        service
            .tokens()
            .sign_in(user, &key.thumbprint(), now)
            .unwrap()
    };
    let laptop = sign_in("mike", &laptop_key);
    let desktop = sign_in("mike", &desktop_key);
    let brett = sign_in("brett", &brett_key);

    let (status, body) = revoke(&url, Some((&laptop_key, &laptop.access_token)), None).await;
    assert_eq!(status, 200, "{body}");
    let revoked: Revoked = serde_json::from_str(&body).unwrap();
    assert_eq!((revoked.user.as_str(), revoked.sign_ins), ("mike", 2));
    let check = |token: &str, key: &Key| service.tokens().check(token, &key.thumbprint(), now);
    assert!(check(&desktop.access_token, &desktop_key).is_err());
    assert_eq!(
        check(&brett.access_token, &brett_key),
        Ok("brett".to_owned())
    );

    // The revoked token no longer works here either.
    let (status, _) = revoke(&url, Some((&laptop_key, &laptop.access_token)), None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn only_an_admin_revokes_another_person() {
    let (service, url) = common::start(false, &["mike"]).await;
    let now = Instant::now();
    let (mike_key, brett_key) = (Key::generate(), Key::generate());
    let mike = service
        .tokens()
        .sign_in("mike", &mike_key.thumbprint(), now)
        .unwrap();
    let brett = service
        .tokens()
        .sign_in("brett", &brett_key.thumbprint(), now)
        .unwrap();

    let (status, _) = revoke(&url, Some((&brett_key, &brett.access_token)), Some("mike")).await;
    assert_eq!(status, 403);

    let (status, body) = revoke(&url, Some((&mike_key, &mike.access_token)), Some("brett")).await;
    assert_eq!(status, 200, "{body}");
    let tokens = service.tokens();
    assert!(
        tokens
            .check(&brett.access_token, &brett_key.thumbprint(), now)
            .is_err()
    );
    assert_eq!(
        tokens.check(&mike.access_token, &mike_key.thumbprint(), now),
        Ok("mike".to_owned())
    );
}

#[tokio::test]
async fn admin_names_ignore_case_and_spaces() {
    let (service, url) = common::start(false, &[" Mike"]).await;
    let now = Instant::now();
    let (mike_key, brett_key) = (Key::generate(), Key::generate());
    let mike = service
        .tokens()
        .sign_in("mike", &mike_key.thumbprint(), now)
        .unwrap();
    service
        .tokens()
        .sign_in("brett", &brett_key.thumbprint(), now)
        .unwrap();

    let (status, body) = revoke(&url, Some((&mike_key, &mike.access_token)), Some("Brett ")).await;
    assert_eq!(status, 200, "{body}");
    let revoked: Revoked = serde_json::from_str(&body).unwrap();
    assert_eq!((revoked.user.as_str(), revoked.sign_ins), ("brett", 1));
}

#[tokio::test]
async fn revoke_needs_a_live_token() {
    let (_, url) = common::start(false, &[]).await;
    assert_eq!(revoke(&url, None, None).await.0, 401);
    assert_eq!(
        revoke(&url, Some((&Key::generate(), "nope")), None).await.0,
        401
    );
}
