//! `riff logout --all` against a real server (R20, R101). The sign-in
//! is in the mock store of `keyring-core`, so the tests run in process.

use std::sync::Once;
use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::dpop::Key;
use riff_core::wire::TokenReply;
use riff_server::Service;
use riff_server::auth::Config;

static MOCK_KEYRING: Once = Once::new();

async fn start(admins: &[&str]) -> (Service, Api) {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        admins: admins.iter().map(|a| a.to_string()).collect(),
        ..Config::new(&url)
    });
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// The device key of this machine for the server of `api`.
fn this_device(api: &Api) -> String {
    riff::device::key(api.base()).unwrap().thumbprint()
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
    let laptop = service
        .tokens()
        .sign_in("mike", &this_device(&api), now)
        .unwrap();
    let desktop_key = Key::generate().thumbprint();
    let desktop = service.tokens().sign_in("mike", &desktop_key, now).unwrap();
    keep(&api, &laptop);

    let done = login::logout_all(&api, None).await.unwrap();
    assert_eq!(
        text::revoked(&done),
        "Ended 2 sign-ins of mike. Each device of mike must sign in again."
    );
    let tokens = service.tokens();
    assert!(
        tokens
            .check(&desktop.access_token, &desktop_key, now)
            .is_err()
    );
    assert_eq!(login::stored(api.base()).unwrap(), None);
}

#[tokio::test]
async fn an_admin_logs_out_another_person() {
    let (service, api) = start(&["mike"]).await;
    let now = Instant::now();
    let jkt = this_device(&api);
    let mike = service.tokens().sign_in("mike", &jkt, now).unwrap();
    let brett = service.tokens().sign_in("brett", &jkt, now).unwrap();

    keep(&api, &brett);
    let error = login::logout_all(&api, Some("mike")).await.unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");

    keep(&api, &mike);
    let done = login::logout_all(&api, Some("brett")).await.unwrap();
    assert!(text::revoked(&done).starts_with("Ended 1 sign-in of brett."));
    let tokens = service.tokens();
    assert!(tokens.check(&brett.access_token, &jkt, now).is_err());
    assert_eq!(tokens.check(&mike.access_token, &jkt, now), Ok("mike"));
    drop(tokens);
    // The admin stays signed in on this device.
    assert!(login::stored(api.base()).unwrap().is_some());
}

#[tokio::test]
async fn a_token_from_another_device_cannot_log_out() {
    let (service, api) = start(&[]).await;
    let other = Key::generate().thumbprint();
    let stolen = service
        .tokens()
        .sign_in("mike", &other, Instant::now())
        .unwrap();
    keep(&api, &stolen);
    assert!(login::logout_all(&api, None).await.is_err());
    let tokens = service.tokens();
    let still = tokens.check(&stolen.access_token, &other, Instant::now());
    assert_eq!(still, Ok("mike"));
}

#[tokio::test]
async fn logout_all_needs_a_sign_in() {
    let (_, api) = start(&[]).await;
    let error = login::logout_all(&api, None).await.unwrap_err();
    assert!(error.to_string().contains("run riff login"), "{error}");
}
