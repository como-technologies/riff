//! `riff cloud smoke` end to end: a fake provider that knows the
//! refresh token of a test account, and a real `riff-server`
//! (01M496JTHN19BZ7YN94993R35X).

use crate::common;

use common::{CLIENT, FakeProvider, REFRESH};
use riff::api::Api;
use riff::{login, smoke};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};

/// A server with sign-in, whose provider signs in Ada, and its URL.
async fn start() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = common::fresh_url(&listener);
    let issuer = FakeProvider::start("Ada@comotechnologies.io", Some(DEFAULT_DOMAIN))
        .await
        .issuer;
    let service = Service::new(Config {
        provider: Some(Provider {
            issuer,
            client_id: CLIENT.into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    url
}

#[tokio::test]
async fn the_smoke_test_signs_in_posts_reads_and_checks_the_build() {
    let url = start().await;
    let mut lines = Vec::new();
    smoke::run(&Api::new(&url), REFRESH, |line| lines.push(line.to_owned()))
        .await
        .unwrap();
    let steps: Vec<&str> = lines.iter().map(|l| l.split_once(':').unwrap().0).collect();
    assert_eq!(
        steps,
        ["sign in", "post", "read", "build", "end"],
        "{lines:?}"
    );
    assert_eq!(lines[0], format!("sign in: ada at {url}"));
    assert!(lines[1].starts_with("post: smoke test "), "{lines:?}");
    assert!(lines[1].ends_with(" to riff/smoke"), "{lines:?}");
    assert!(lines[3].contains(env!("CARGO_PKG_VERSION")), "{lines:?}");
    // The sign-in is kept for the server, as `riff login` keeps it.
    assert_eq!(login::user(&url).unwrap().as_deref(), Some("ada"));

    // A second run posts a new message.
    let mut again = Vec::new();
    smoke::run(&Api::new(&url), REFRESH, |line| again.push(line.to_owned()))
        .await
        .unwrap();
    assert_ne!(lines[1], again[1]);
}

#[tokio::test]
async fn a_refresh_token_that_the_provider_refuses_stops_at_the_sign_in() {
    let url = start().await;
    let mut lines = Vec::new();
    let error = smoke::run(&Api::new(&url), "not-the-token", |line| {
        lines.push(line.to_owned())
    })
    .await
    .unwrap_err();
    let text = format!("{error:#}");
    assert!(text.starts_with("smoke test: sign in: "), "{text}");
    assert!(
        text.contains("the sign-in provider refused the refresh token"),
        "{text}"
    );
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(login::user(&url).unwrap(), None);
}
