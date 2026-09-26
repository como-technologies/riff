//! `riff logout --all` against a real server (R20, R101). The sign-in
//! is in the mock store of `keyring-core`, so the tests run in process.

use std::sync::Once;
use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::wire::TokenReply;
use riff_server::Service;
use riff_server::auth::Config;

static MOCK_KEYRING: Once = Once::new();

async fn start(admins: &[&str]) -> (Service, Api) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let service = Service::new(Config {
        admins: admins.iter().map(|a| a.to_string()).collect(),
        ..Config::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = Api::new(&format!("http://{}", listener.local_addr().unwrap()));
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// Keeps `pair` as the sign-in of this device.
fn keep(api: &Api, pair: &TokenReply) {
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        expires_at: u64::MAX,
    };
    login::store(api.base(), &sign_in).unwrap();
}

#[tokio::test]
async fn logout_all_ends_each_sign_in_of_the_caller() {
    let (service, api) = start(&[]).await;
    let now = Instant::now();
    let laptop = service.tokens().sign_in("mike", now).unwrap();
    let desktop = service.tokens().sign_in("mike", now).unwrap();
    keep(&api, &laptop);

    let done = login::logout_all(&api, None).await.unwrap();
    assert_eq!(
        text::revoked(&done),
        "Ended 2 sign-ins of mike. Each device of mike must sign in again."
    );
    assert!(service.tokens().check(&desktop.access_token, now).is_err());
    assert_eq!(login::stored(api.base()).unwrap(), None);
}

#[tokio::test]
async fn an_admin_logs_out_another_person() {
    let (service, api) = start(&["mike"]).await;
    let now = Instant::now();
    let mike = service.tokens().sign_in("mike", now).unwrap();
    let brett = service.tokens().sign_in("brett", now).unwrap();

    keep(&api, &brett);
    let error = login::logout_all(&api, Some("mike")).await.unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");

    keep(&api, &mike);
    let done = login::logout_all(&api, Some("brett")).await.unwrap();
    assert!(text::revoked(&done).starts_with("Ended 1 sign-in of brett."));
    assert!(service.tokens().check(&brett.access_token, now).is_err());
    assert_eq!(service.tokens().check(&mike.access_token, now), Ok("mike"));
    // The admin stays signed in on this device.
    assert!(login::stored(api.base()).unwrap().is_some());
}

#[tokio::test]
async fn logout_all_needs_a_sign_in() {
    let (_, api) = start(&[]).await;
    let error = login::logout_all(&api, None).await.unwrap_err();
    assert!(error.to_string().contains("run riff login"), "{error}");
}
