//! The MCP authorization spec, over HTTP (R22).

use std::time::Instant;

use riff_core::wire::{ResourceMetadata, ServerMetadata, TokenError};
use riff_server::Service;
use riff_server::auth::Config;

async fn start(require_sign_in: bool) -> (Service, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut config = Config::new(&url);
    config.require_sign_in = require_sign_in;
    let service = Service::new(config);
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

async fn who(url: &str, token: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new()
        .post(format!("{url}/v1/who"))
        .header("content-type", "application/json")
        .body("{}");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request.send().await.unwrap()
}

fn challenge(reply: &reqwest::Response) -> String {
    reply.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn discovery_leads_from_a_401_to_the_token_endpoint() {
    let (_, url) = start(true).await;

    // 1. No token: 401 names the resource metadata.
    let reply = who(&url, None).await;
    assert_eq!(reply.status(), 401);
    let metadata_url = format!("{url}/.well-known/oauth-protected-resource");
    assert_eq!(
        challenge(&reply),
        format!(r#"Bearer resource_metadata="{metadata_url}""#)
    );

    // 2. The resource metadata names the issuer.
    let resource: ResourceMetadata = reqwest::get(&metadata_url)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resource.resource, url);
    assert_eq!(resource.authorization_servers, [url.as_str()]);

    // 3. The issuer metadata names the token endpoint.
    let issuer = &resource.authorization_servers[0];
    let server: ServerMetadata =
        reqwest::get(format!("{issuer}/.well-known/oauth-authorization-server"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(server.issuer, *issuer);
    assert_eq!(server.token_endpoint, format!("{url}/v1/token"));
    assert!(
        server
            .grant_types_supported
            .contains(&"refresh_token".into())
    );
    assert_eq!(server.code_challenge_methods_supported, ["S256"]);
}

#[tokio::test]
async fn a_live_token_passes_and_a_bad_one_gets_invalid_token() {
    let (service, url) = start(true).await;
    let pair = service.tokens().sign_in("mike", Instant::now()).unwrap();

    assert_eq!(who(&url, Some(&pair.access_token)).await.status(), 200);

    let reply = who(&url, Some("nope")).await;
    assert_eq!(reply.status(), 401);
    assert!(challenge(&reply).starts_with(r#"Bearer error="invalid_token""#));
}

#[tokio::test]
async fn a_token_in_the_query_string_does_not_count() {
    let (service, url) = start(true).await;
    let pair = service.tokens().sign_in("mike", Instant::now()).unwrap();
    let reply = reqwest::Client::new()
        .post(format!("{url}/v1/who?access_token={}", pair.access_token))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

#[tokio::test]
async fn without_the_setting_no_token_is_needed() {
    let (_, url) = start(false).await;
    assert_eq!(who(&url, None).await.status(), 200);
}

#[tokio::test]
async fn the_token_endpoint_checks_the_resource() {
    let (service, url) = start(true).await;
    let pair = service.tokens().sign_in("mike", Instant::now()).unwrap();
    let refresh = |resource: String, token: String| {
        let url = url.clone();
        async move {
            let form =
                format!("grant_type=refresh_token&refresh_token={token}&resource={resource}");
            reqwest::Client::new()
                .post(format!("{url}/v1/token"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(form)
                .send()
                .await
                .unwrap()
        }
    };

    let other = refresh(
        "https://evil.example.com".into(),
        pair.refresh_token.clone(),
    )
    .await;
    assert_eq!(other.status(), 400);
    let error: TokenError = other.json().await.unwrap();
    assert_eq!(error.error, "invalid_target");

    // The refused request did not use the refresh token.
    let ours = refresh(url.clone(), pair.refresh_token).await;
    assert_eq!(ours.status(), 200);
}
