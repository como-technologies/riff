//! `GET /v1/sign-in` and the token exchange, with a fake provider that
//! serves a discovery document and a JWKS.

mod common;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use riff_core::dpop::Key;
use riff_core::wire::{
    ID_TOKEN_TYPE, SignInConfig, TOKEN_EXCHANGE, TokenError, TokenReply, TokenRequest,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};

/// A token exchange with a proof from `key`, or with no proof.
async fn exchange_with(server: &str, token: &str, key: Option<&Key>) -> reqwest::Response {
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(token.into()),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let url = format!("{server}/v1/token");
    let mut request = common::client().post(&url).form(&form);
    if let Some(key) = key {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        request = request.header("dpop", key.proof("POST", &url, None, now));
    }
    request.send().await.unwrap()
}

async fn exchange(server: &str, token: &str) -> reqwest::Response {
    exchange_with(server, token, Some(&Key::generate())).await
}

async fn start() -> (Service, String, String) {
    let issuer = common::fake_provider().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        provider: Some(Provider {
            issuer: issuer.clone(),
            client_id: "riff-client".into(),
            client_secret: Some("not-secret".into()),
            allowed_domains: vec![DEFAULT_DOMAIN.into()],
        }),
        ..Config::new(&server)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, server, issuer)
}

#[tokio::test]
async fn sign_in_names_the_provider() {
    let (_, server, issuer) = start().await;
    let config: SignInConfig = common::client()
        .get(format!("{server}/v1/sign-in"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(config.issuer, issuer);
    assert_eq!(config.client_id, "riff-client");
    assert_eq!(config.client_secret.as_deref(), Some("not-secret"));
}

#[tokio::test]
async fn an_id_token_gives_riff_tokens_for_its_user() {
    let (service, server, issuer) = start().await;
    let key = Key::generate();
    let reply = exchange_with(
        &server,
        &common::id_token(&issuer, "Ada@comotechnologies.io", Some(DEFAULT_DOMAIN)),
        Some(&key),
    )
    .await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.headers()["cache-control"], "no-store");
    let pair: TokenReply = reply.json().await.unwrap();
    assert_eq!(pair.user, "ada");
    assert_eq!(
        service
            .tokens()
            .check(&pair.access_token, &key.thumbprint(), Instant::now()),
        Ok("ada".to_owned())
    );
}

#[tokio::test]
async fn an_exchange_without_a_proof_is_refused() {
    let (_, server, issuer) = start().await;
    let token = common::id_token(&issuer, "ada@comotechnologies.io", Some(DEFAULT_DOMAIN));
    let reply = exchange_with(&server, &token, None).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "invalid_dpop_proof");
}

#[tokio::test]
async fn a_bad_id_token_is_refused() {
    let (_, server, _) = start().await;
    let other = common::id_token(
        "https://other.test",
        "ada@comotechnologies.io",
        Some(DEFAULT_DOMAIN),
    );
    let reply = exchange(&server, &other).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "invalid_grant");
}

#[tokio::test]
async fn a_server_without_a_provider_has_no_sign_in() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let router = Service::new(Config::new(&server)).router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let reply = common::client()
        .get(format!("{server}/v1/sign-in"))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 404);
    let reply = exchange(&server, "any").await;
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "unsupported_grant_type");
}

/// Two emails that give the same USER: only the first one gets it
/// (R209).
#[tokio::test]
async fn a_second_email_does_not_get_the_user_of_the_first() {
    let (service, server, issuer) = start().await;
    let first = common::id_token(&issuer, "O'Brien@comotechnologies.io", Some(DEFAULT_DOMAIN));
    let reply = exchange(&server, &first).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    assert_eq!(pair.user, "o-brien");

    // The other account gives the same USER. The server refuses it.
    let second = common::id_token(&issuer, "o-brien@comotechnologies.io", Some(DEFAULT_DOMAIN));
    let reply = exchange(&server, &second).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "access_denied");

    let why = error.error_description.unwrap();
    assert!(
        why.contains("the user o-brien belongs to another account"),
        "{why}"
    );

    // The first email holds the USER: it signs in again, on another
    // device.
    let reply = exchange(&server, &first).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    assert_eq!(pair.user, "o-brien");
    // Only the two sign-ins of the first email are there.
    assert_eq!(service.tokens().revoke_user("o-brien"), 2);
}

/// Each allowed domain has its own accounts, and a USER belongs to one
/// of them (R209).
#[tokio::test]
async fn two_domains_do_not_share_a_user() {
    let issuer = common::fake_provider().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        provider: Some(Provider {
            issuer: issuer.clone(),
            client_id: "riff-client".into(),
            client_secret: None,
            allowed_domains: vec![DEFAULT_DOMAIN.into(), "other.test".into()],
        }),
        ..Config::new(&server)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let first = common::id_token(&issuer, "alice@comotechnologies.io", Some(DEFAULT_DOMAIN));
    let reply = exchange(&server, &first).await;
    assert_eq!(reply.status(), 200);
    let pair: TokenReply = reply.json().await.unwrap();
    assert_eq!(pair.user, "alice");

    let second = common::id_token(&issuer, "alice@other.test", Some("other.test"));
    let reply = exchange(&server, &second).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "access_denied");
    let why = error.error_description.unwrap();
    assert!(
        why.contains("the user alice belongs to another account"),
        "{why}"
    );

    // The account of the first domain holds the USER: it signs in
    // again. Only its two sign-ins are there.
    assert_eq!(exchange(&server, &first).await.status(), 200);
    assert_eq!(service.tokens().revoke_user("alice"), 2);
}

/// An account from another domain, with no invite, is refused once
/// the riff has an owner (R15).
#[tokio::test]
async fn an_account_from_another_domain_is_refused() {
    let (service, server, issuer) = start().await;
    let owner = common::id_token(&issuer, "owner@comotechnologies.io", Some(DEFAULT_DOMAIN));
    assert_eq!(exchange(&server, &owner).await.status(), 200);
    let reply = exchange(
        &server,
        &common::id_token(&issuer, "ada@gmail.com", Some("gmail.com")),
    )
    .await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "access_denied");
    let why = error.error_description.unwrap();
    assert!(why.contains("riff invite ada@gmail.com"), "{why}");
    // No sign-in started.
    assert_eq!(service.tokens().revoke_user("ada"), 0);
}
