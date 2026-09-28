//! A riff that starts again with no sign-in (R226, R227). This machine
//! keeps its old sign-in for that riff. No error says `riff login`: the
//! error says `riff logout`, and after it each call works. The keyring
//! is the mock store of `keyring-core`.

use std::net::SocketAddr;
use std::sync::Once;
use std::time::Instant;

use riff::api::Api;
use riff::login::{self, SignIn};
use riff::text;
use riff_core::name::SessionUri;
use riff_server::Service;
use riff_server::auth::Config;
use tokio::task::JoinHandle;

static MOCK_KEYRING: Once = Once::new();

fn mock_keyring() {
    MOCK_KEYRING.call_once(|| {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
    });
}

/// Serves `service` at `addr`.
async fn serve(service: &Service, addr: SocketAddr) -> JoinHandle<()> {
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() })
}

fn me(user: &str) -> SessionUri {
    format!("riff://{user}@pangolin/como-technologies/riff?session=s1")
        .parse()
        .unwrap()
}

/// A riff with sign-in where this machine signs in as mike. Then the
/// riff starts again at the same URL with no sign-in and no tokens.
/// The old sign-in stays in the keyring. `expired` makes its access
/// token old, so the next person call needs a refresh.
async fn restart_with_no_sign_in(expired: bool) -> String {
    mock_keyring();
    let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = free.local_addr().unwrap();
    drop(free);
    let url = format!("http://{addr}");
    let first = Service::new(Config::new(&url));
    let running = serve(&first, addr).await;
    let jkt = riff::device::key(&url).unwrap().thumbprint();
    let pair = first
        .tokens()
        .sign_in("mike@comotechnologies.io", &jkt, Instant::now())
        .unwrap();
    let sign_in = SignIn {
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_at: u64::MAX,
        riff_id: None,
    };
    login::store(&url, &sign_in).unwrap();
    let api = Api::new(&url).signed_in(Some("s1")).unwrap();
    api.register(&me("mike")).await.unwrap();
    if expired {
        let old = SignIn {
            expires_at: 0,
            riff_id: None,
            ..sign_in
        };
        login::store(&url, &old).unwrap();
    }
    running.abort();
    let _ = running.await;

    let second = Service::default();
    assert!(!second.config().require_sign_in);
    serve(&second, addr).await;
    url
}

fn assert_says_logout(error: &anyhow::Error, url: &str, kept: bool) {
    let error = format!("{error:#}");
    assert_eq!(error, text::no_sign_in(url, kept));
    assert!(!error.contains("riff login"), "{error}");
}

#[tokio::test]
async fn a_session_at_a_riff_with_no_sign_in_says_to_run_riff_logout() {
    let url = restart_with_no_sign_in(false).await;
    let api = Api::new(&url);
    assert!(!api.has_sign_in().await.unwrap());
    let session = api.clone().signed_in(Some("s1")).unwrap();
    let error = session.who(&me("mike"), false).await.unwrap_err();
    assert_says_logout(&error, &url, true);

    // riff logout, as the error says. Then each new client works.
    assert!(login::logout(&url).unwrap());
    let fresh = api.clone().signed_in(Some("s1")).unwrap();
    let shown = fresh.who(&me("sandman"), false).await.unwrap();
    assert_eq!(shown.len(), 1);

    // A client that started with the old sign-in says to start again.
    let error = session.who(&me("mike"), false).await.unwrap_err();
    assert_says_logout(&error, &url, false);
}

#[tokio::test]
async fn a_person_whose_token_needs_a_refresh_is_told_to_run_riff_logout() {
    let url = restart_with_no_sign_in(true).await;
    let person = Api::new(&url).signed_in(None).unwrap();
    let error = person.who(&me("mike"), false).await.unwrap_err();
    assert_says_logout(&error, &url, true);
}

#[tokio::test]
async fn a_riff_that_cannot_be_reached_keeps_the_first_error() {
    mock_keyring();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    login::store(
        &url,
        &SignIn {
            user: "mike".into(),
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 0,
            riff_id: None,
        },
    )
    .unwrap();
    let person = Api::new(&url).signed_in(None).unwrap();
    let error = person.who(&me("mike"), false).await.unwrap_err();
    let error = format!("{error:#}");
    // Only a refused grant ends the sign-in (01M3MX4TSEH18FSNQ28GEH2GFJ).
    assert!(!error.contains("the sign-in ended"), "{error}");
    assert!(error.contains("cannot reach riff-server"), "{error}");
}
