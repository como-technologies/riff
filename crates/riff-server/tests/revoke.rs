//! `POST /v1/revoke` over HTTP (R20). Each riff here has sign-in: a
//! riff with no sign-in has no people.

mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{Revoke, Revoked};

async fn revoke(base: &str, auth: Option<(&Key, &str)>, user: Option<&str>) -> (u16, String) {
    let url = format!("{base}/v1/revoke");
    let request = match auth {
        Some((key, token)) => common::post(&url, key, Some(token)),
        None => common::client().post(&url),
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
    let (service, url) = common::start(true, &[]).await;
    let now = Instant::now();
    let (laptop_key, desktop_key, brett_key) = (Key::generate(), Key::generate(), Key::generate());
    let sign_in = |email, key: &Key| {
        service
            .tokens()
            .sign_in(email, &key.thumbprint(), now)
            .unwrap()
    };
    let laptop = sign_in("mike@comotechnologies.io", &laptop_key);
    let desktop = sign_in("mike@comotechnologies.io", &desktop_key);
    let brett = sign_in("brett@comotechnologies.io", &brett_key);

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
    // Given a riff whose settings name mike as an admin. brett is of
    // an allowed domain, and is no admin.
    let (service, url) = common::start(true, &["mike@comotechnologies.io"]).await;
    let (mike_key, brett_key) = (Key::generate(), Key::generate());
    let mike = service
        .admit("mike@comotechnologies.io", false, &mike_key.thumbprint())
        .await
        .unwrap();
    let brett = service
        .admit("brett@comotechnologies.io", true, &brett_key.thumbprint())
        .await
        .unwrap();

    // When a person who is no admin names another person: 403.
    let (status, body) = revoke(&url, Some((&brett_key, &brett.access_token)), Some("mike")).await;
    assert_eq!(status, 403);
    assert!(
        body.contains("only an admin revokes another person"),
        "{body}"
    );

    // When the admin names another person, each sign-in of that person
    // ends. The sign-in of the admin stays.
    let (status, body) = revoke(&url, Some((&mike_key, &mike.access_token)), Some("brett")).await;
    assert_eq!(status, 200, "{body}");
    let now = Instant::now();
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
async fn admin_emails_ignore_case_and_spaces() {
    let (service, url) = common::start(true, &[" Mike@ComoTechnologies.io "]).await;
    let (mike_key, brett_key) = (Key::generate(), Key::generate());
    let mike = service
        .admit("mike@comotechnologies.io", false, &mike_key.thumbprint())
        .await
        .unwrap();
    service
        .admit("brett@comotechnologies.io", true, &brett_key.thumbprint())
        .await
        .unwrap();

    let (status, body) = revoke(&url, Some((&mike_key, &mike.access_token)), Some("Brett ")).await;
    assert_eq!(status, 200, "{body}");
    let revoked: Revoked = serde_json::from_str(&body).unwrap();
    assert_eq!((revoked.user.as_str(), revoked.sign_ins), ("brett", 1));
}

/// An admin is named by verified email (R210). Another account with
/// the same USER as the admin is not an admin.
#[tokio::test]
async fn only_the_email_of_the_admin_is_an_admin() {
    let (service, url) = common::start(true, &["alice@a.test"]).await;
    let (alice_key, brett_key) = (Key::generate(), Key::generate());
    // This Alice signs in as `alice`, with another email than the admin
    // of the settings. Each account is of an allowed domain.
    let alice = service
        .admit("alice@b.test", true, &alice_key.thumbprint())
        .await
        .unwrap();
    assert_eq!(alice.user, "alice");
    service
        .admit("brett@a.test", true, &brett_key.thumbprint())
        .await
        .unwrap();

    let (status, body) = revoke(&url, Some((&alice_key, &alice.access_token)), Some("brett")).await;
    assert_eq!(status, 403, "{body}");
    // Her own sign-ins are still hers to end.
    let (status, body) = revoke(&url, Some((&alice_key, &alice.access_token)), None).await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn revoke_needs_a_live_token() {
    let (_, url) = common::start(true, &[]).await;
    assert_eq!(revoke(&url, None, None).await.0, 401);
    assert_eq!(
        revoke(&url, Some((&Key::generate(), "nope")), None).await.0,
        401
    );
}
