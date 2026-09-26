//! `GET /v1/sign-in` and the token exchange, with a fake provider that
//! serves a discovery document and a JWKS.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use riff_core::dpop::Key;
use riff_core::wire::{
    Discovery, ID_TOKEN_TYPE, SignInConfig, TOKEN_EXCHANGE, TokenError, TokenReply, TokenRequest,
};
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::oidc::{DEFAULT_DOMAIN, Provider};
use serde_json::{Value, json};

const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../testdata/test-only-jwks.json");

/// A provider with only the two documents that the server fetches.
async fn fake_provider() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = Discovery {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{issuer}/authorize"),
        token_endpoint: format!("{issuer}/token"),
        jwks_uri: format!("{issuer}/jwks"),
    };
    let jwks: Value = serde_json::from_str(JWKS).unwrap();
    let router = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || async move { Json(discovery) }),
        )
        .route("/jwks", get(move || async move { Json(jwks) }));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    issuer
}

fn id_token(issuer: &str, email: &str, domain: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test".into());
    let claims = json!({
        "iss": issuer,
        "aud": "riff-client",
        "exp": now + 3600,
        "email": email,
        "email_verified": true,
        "hd": domain,
    });
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}

/// A token exchange with a proof from `key`, or with no proof.
async fn exchange_with(server: &str, token: &str, key: Option<&Key>) -> reqwest::Response {
    let form = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(token.into()),
        subject_token_type: Some(ID_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    let url = format!("{server}/v1/token");
    let mut request = reqwest::Client::new().post(&url).form(&form);
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
    let issuer = fake_provider().await;
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
    let config: SignInConfig = reqwest::get(format!("{server}/v1/sign-in"))
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
        &id_token(&issuer, "Mike@comotechnologies.io", DEFAULT_DOMAIN),
        Some(&key),
    )
    .await;
    assert_eq!(reply.status(), 200);
    assert_eq!(reply.headers()["cache-control"], "no-store");
    let pair: TokenReply = reply.json().await.unwrap();
    assert_eq!(pair.user, "mike");
    assert_eq!(
        service
            .tokens()
            .check(&pair.access_token, &key.thumbprint(), Instant::now()),
        Ok("mike")
    );
}

#[tokio::test]
async fn an_exchange_without_a_proof_is_refused() {
    let (_, server, issuer) = start().await;
    let token = id_token(&issuer, "mike@comotechnologies.io", DEFAULT_DOMAIN);
    let reply = exchange_with(&server, &token, None).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "invalid_dpop_proof");
}

#[tokio::test]
async fn a_bad_id_token_is_refused() {
    let (_, server, _) = start().await;
    let other = id_token(
        "https://other.test",
        "mike@comotechnologies.io",
        DEFAULT_DOMAIN,
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
    let reply = reqwest::get(format!("{server}/v1/sign-in")).await.unwrap();
    assert_eq!(reply.status(), 404);
    let reply = exchange(&server, "any").await;
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "unsupported_grant_type");
}

#[tokio::test]
async fn an_account_from_another_domain_is_refused() {
    let (service, server, issuer) = start().await;
    let reply = exchange(&server, &id_token(&issuer, "mike@gmail.com", "gmail.com")).await;
    assert_eq!(reply.status(), 400);
    let error: TokenError = reply.json().await.unwrap();
    assert_eq!(error.error, "invalid_grant");
    // No sign-in started.
    assert_eq!(service.tokens().revoke_user("mike"), 0);
}
