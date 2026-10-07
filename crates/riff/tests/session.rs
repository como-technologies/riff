//! A token for each session, from `riff` to a real server that needs
//! sign-in (R19, R104, R107). The sign-in is in the mock store of
//! `keyring-core`.

use crate::common;

use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff_core::name::SessionUri;
use riff_server::Service;
use riff_server::auth::Config;

/// A server that needs sign-in, and a sign-in of mike on this device.
/// `expires_at` is when the kept access token expires.
async fn start(expires_at: u64) -> (Service, Api) {
    let (listener, url) = common::listen().await;
    let service = Service::new(Config {
        require_sign_in: true,
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let api = Api::new(&url);
    let jkt = riff::device::key(api.base()).unwrap().thumbprint();
    let pair = service
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let sign_in = SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at,
        riff_id: None,
    };
    login::store(api.base(), &sign_in).unwrap();
    (service, api)
}

fn uri(s: &str) -> SessionUri {
    s.parse().unwrap()
}

#[tokio::test]
async fn a_session_client_acts_only_as_its_session() {
    let (_, api) = start(u64::MAX).await;
    let a = uri("riff://mike@pangolin/como-technologies/riff?session=a");
    let b = uri("riff://mike@pangolin/como-technologies/riff?session=b");
    let mike = uri("riff://mike@pangolin");

    let session = api.clone().signed_in(Some("a")).unwrap();
    session.register(&a).await.unwrap();
    // The second call uses the session token that the client holds.
    session.register(&a).await.unwrap();
    let error = session.register(&b).await.unwrap_err();
    assert!(error.to_string().contains("403"), "{error}");
    assert!(session.register(&mike).await.is_err());

    let person = api.clone().signed_in(None).unwrap();
    person.register(&mike).await.unwrap();
    assert!(person.register(&a).await.is_err());

    // Without a token, the server refuses each call.
    let error = api.register(&mike).await.unwrap_err();
    assert!(error.to_string().contains("401"), "{error}");
}

#[tokio::test]
async fn processes_that_refresh_at_once_keep_the_sign_in() {
    let (_, api) = start(0).await;
    let (a, b) = tokio::join!(login::access_token(&api), login::access_token(&api));
    // One refresh ran; the other waited and used its pair.
    assert_eq!(a.unwrap(), b.unwrap());
    let person = api.clone().signed_in(None).unwrap();
    person.register(&uri("riff://mike@pangolin")).await.unwrap();
}
