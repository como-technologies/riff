//! `POST /v1/revoke` over HTTP (R20).

use std::time::Instant;

use riff_core::wire::{Revoke, Revoked};
use riff_server::Service;
use riff_server::auth::Config;

async fn start(admins: &[&str]) -> (Service, String) {
    let service = Service::new(Config {
        admins: admins.iter().map(|a| a.to_string()).collect(),
        ..Config::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/revoke", listener.local_addr().unwrap());
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

async fn revoke(url: &str, token: Option<&str>, user: Option<&str>) -> (u16, String) {
    let mut request = reqwest::Client::new().post(url).json(&Revoke {
        user: user.map(str::to_owned),
    });
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let reply = request.send().await.unwrap();
    (reply.status().as_u16(), reply.text().await.unwrap())
}

#[tokio::test]
async fn a_person_revokes_each_of_their_sign_ins() {
    let (service, url) = start(&[]).await;
    let now = Instant::now();
    let laptop = service.tokens().sign_in("mike", now).unwrap();
    let desktop = service.tokens().sign_in("mike", now).unwrap();
    let brett = service.tokens().sign_in("brett", now).unwrap();

    let (status, body) = revoke(&url, Some(&laptop.access_token), None).await;
    assert_eq!(status, 200, "{body}");
    let revoked: Revoked = serde_json::from_str(&body).unwrap();
    assert_eq!((revoked.user.as_str(), revoked.sign_ins), ("mike", 2));
    assert!(service.tokens().check(&desktop.access_token, now).is_err());
    assert_eq!(
        service.tokens().check(&brett.access_token, now),
        Ok("brett")
    );

    // The revoked token no longer works here either.
    let (status, _) = revoke(&url, Some(&laptop.access_token), None).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn only_an_admin_revokes_another_person() {
    let (service, url) = start(&["mike"]).await;
    let now = Instant::now();
    let mike = service.tokens().sign_in("mike", now).unwrap();
    let brett = service.tokens().sign_in("brett", now).unwrap();

    let (status, _) = revoke(&url, Some(&brett.access_token), Some("mike")).await;
    assert_eq!(status, 403);
    assert_eq!(service.tokens().check(&mike.access_token, now), Ok("mike"));

    let (status, body) = revoke(&url, Some(&mike.access_token), Some("brett")).await;
    assert_eq!(status, 200, "{body}");
    assert!(service.tokens().check(&brett.access_token, now).is_err());
    assert_eq!(service.tokens().check(&mike.access_token, now), Ok("mike"));
}

#[tokio::test]
async fn revoke_needs_a_live_token() {
    let (_, url) = start(&[]).await;
    assert_eq!(revoke(&url, None, None).await.0, 401);
    assert_eq!(revoke(&url, Some("nope"), None).await.0, 401);
}
