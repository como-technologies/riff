//! `riff logout --all` against a real server (R20, R101). The sign-in
//! is in the mock store of `keyring-core`, so the tests run in process.
//! A riff here has sign-in, unless the test is about a riff with no
//! sign-in: only a riff with sign-in ends a sign-in.

mod common;

use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::dpop::Key;
use riff_core::wire::TokenReply;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::Provider;

/// A riff with sign-in. `admins` are the admin emails of the settings.
/// No test calls the provider.
async fn start(admins: &[&str]) -> (Service, Api) {
    let provider = Provider {
        issuer: "https://accounts.example.com".into(),
        client_id: "riff".into(),
        client_secret: None,
        allowed_domains: Vec::new(),
    };
    serve(|url| Config {
        admins: admins.iter().map(|a| a.to_string()).collect(),
        provider: Some(provider),
        ..Config::new(url)
    })
    .await
}

/// A server with the config that `config` makes from its URL.
async fn serve(config: impl FnOnce(&str) -> Config) -> (Service, Api) {
    let (listener, url) = common::listen().await;
    let service = Service::new(config(&url));
    let api = Api::new(&url);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, api)
}

/// The device key of this machine for the server of `api`.
fn this_device(api: &Api) -> String {
    riff::device::key(api.base()).unwrap().thumbprint()
}

/// Keeps `pair` as the sign-in of this device at `service`.
fn keep(service: &Service, api: &Api, pair: &TokenReply) {
    let sign_in = SignIn {
        user: pair.user.clone(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        expires_at: u64::MAX,
        // The client checks the riff ID of a riff with sign-in
        // (01M3JNVBRS35B3CD67367JF7SJ).
        riff_id: service.riff_id(),
    };
    login::store(api.base(), &sign_in).unwrap();
}

#[tokio::test]
async fn logout_all_ends_each_sign_in_of_the_caller() {
    let (service, api) = start(&[]).await;
    let now = Instant::now();
    let laptop = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &this_device(&api), now)
        .unwrap();
    let desktop_key = Key::generate().thumbprint();
    let desktop = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &desktop_key, now)
        .unwrap();
    keep(&service, &api, &laptop);

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
    // Given a riff whose settings name mike as an admin. brett is of
    // an allowed domain, and is no admin.
    let (service, api) = start(&["mike@comotechnologies.io"]).await;
    let jkt = this_device(&api);
    let mike = service
        .admit("mike@comotechnologies.io", false, &jkt)
        .await
        .unwrap();
    let brett = service
        .admit("brett@comotechnologies.io", true, &jkt)
        .await
        .unwrap();

    keep(&service, &api, &brett);
    let error = login::logout_all(&api, Some("mike")).await.unwrap_err();
    assert!(error.to_string().contains("not an admin"), "{error}");

    keep(&service, &api, &mike);
    let done = login::logout_all(&api, Some("brett")).await.unwrap();
    assert!(text::revoked(&done).starts_with("Ended 1 sign-in of brett."));
    let now = Instant::now();
    let tokens = service.tokens();
    assert!(tokens.check(&brett.access_token, &jkt, now).is_err());
    assert_eq!(
        tokens.check(&mike.access_token, &jkt, now),
        Ok("mike".to_owned())
    );
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
        .sign_in("mike@comotechnologies.io", &other, Instant::now())
        .unwrap();
    keep(&service, &api, &stolen);
    assert!(login::logout_all(&api, None).await.is_err());
    let tokens = service.tokens();
    let still = tokens.check(&stolen.access_token, &other, Instant::now());
    assert_eq!(still, Ok("mike".to_owned()));
}

/// This server has no sign-in provider, so nobody can sign in (R227).
#[tokio::test]
async fn logout_all_needs_a_sign_in() {
    let (_, api) = serve(Config::new).await;
    let error = login::logout_all(&api, None).await.unwrap_err();
    assert_eq!(error.to_string(), text::nobody_signs_in(api.base()));
}
