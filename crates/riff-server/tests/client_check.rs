//! At start, `riff-server` checks its OAuth client with the provider,
//! and stops when the provider refuses it (R146). A fake provider
//! knows one client, `riff-client` with the secret `right`.

use isolated::Isolated;
use std::collections::HashMap;
use std::time::Duration;

use axum::extract::Form;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use riff_core::wire::Discovery;
use riff_server::oidc::{self, CHECK_CODE, DEFAULT_DOMAIN, Provider, SignInError};
use serde_json::{Value, json};

/// The reply of the fake token endpoint, like Google's.
async fn token(Form(form): Form<HashMap<String, String>>) -> (StatusCode, Json<Value>) {
    let field = |name: &str| form.get(name).map(String::as_str);
    if field("client_id") != Some("riff-client") {
        let error = json!({"error": "invalid_client", "error_description": "The OAuth client was not found."});
        return (StatusCode::UNAUTHORIZED, Json(error));
    }
    if field("client_secret") != Some("right") {
        let error = json!({"error": "invalid_client", "error_description": "The provided client secret is invalid."});
        return (StatusCode::UNAUTHORIZED, Json(error));
    }
    assert_eq!(field("code"), Some(CHECK_CODE));
    let error = json!({"error": "invalid_grant", "error_description": "Malformed auth code."});
    (StatusCode::BAD_REQUEST, Json(error))
}

/// A provider whose token endpoint is `token_path`.
async fn fake_provider(token_path: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let discovery = Discovery {
        issuer: issuer.clone(),
        authorization_endpoint: format!("{issuer}/authorize"),
        token_endpoint: format!("{issuer}{token_path}"),
        jwks_uri: format!("{issuer}/jwks"),
    };
    let router = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || async move { Json(discovery) }),
        )
        .route("/token", post(token))
        .route("/busy", post(|| async { StatusCode::SERVICE_UNAVAILABLE }));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    issuer
}

fn provider(issuer: &str, client_id: &str, secret: Option<&str>) -> Provider {
    Provider {
        issuer: issuer.into(),
        client_id: client_id.into(),
        client_secret: secret.map(str::to_owned),
        allowed_domains: vec![DEFAULT_DOMAIN.into()],
    }
}

async fn check(provider: &Provider) -> Result<(), SignInError> {
    provider
        .check_client(&oidc::client(Duration::from_secs(5)))
        .await
}

#[tokio::test]
async fn the_provider_knows_the_client() {
    let issuer = fake_provider("/token").await;
    check(&provider(&issuer, "riff-client", Some("right")))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_provider_refuses_an_unknown_client_or_a_wrong_secret() {
    let issuer = fake_provider("/token").await;
    for (id, secret) in [("other", Some("right")), ("riff-client", Some("wrong"))] {
        let error = check(&provider(&issuer, id, secret)).await.unwrap_err();
        assert!(matches!(error, SignInError::Client(_)), "{error}");
        assert!(error.to_string().contains("invalid_client"), "{error}");
    }
}

#[tokio::test]
async fn a_busy_provider_does_not_refuse_the_client() {
    let issuer = fake_provider("/busy").await;
    let error = check(&provider(&issuer, "riff-client", Some("right")))
        .await
        .unwrap_err();
    assert!(matches!(error, SignInError::Provider(_)), "{error}");
}

#[tokio::test]
async fn the_server_stops_when_the_provider_refuses_the_client() {
    let issuer = fake_provider("/token").await;
    let mut cmd = Isolated::shared().assert_riff_server();
    cmd.args(["--listen", "127.0.0.1:0", "--issuer", &issuer])
        .args(["--client-id", "riff-client", "--client-secret", "wrong"])
        .env_remove("RIFF_OIDC_CLIENT_SECRET")
        .timeout(isolated::HANG);
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(log.contains("refuses the OAuth client"), "{log}");
}
