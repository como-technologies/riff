//! The MCP authorization spec, over HTTP (R22).

mod common;

use std::time::Instant;

use riff_core::dpop::Key;
use riff_core::wire::{ResourceMetadata, ServerMetadata, TokenError};

async fn who(base: &str, auth: Option<(&Key, &str)>) -> reqwest::Response {
    let url = format!("{base}/v1/who");
    let request = match auth {
        Some((key, token)) => common::post(&url, key, Some(token)),
        None => reqwest::Client::new().post(&url),
    };
    request
        .header("content-type", "application/json")
        .body(r#"{"me":"riff://mike@pangolin"}"#)
        .send()
        .await
        .unwrap()
}

fn challenge(reply: &reqwest::Response) -> String {
    reply.headers()["www-authenticate"]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn discovery_leads_from_a_401_to_the_token_endpoint() {
    let (_, url) = common::start(true, &[]).await;

    // 1. No token: 401 names the resource metadata.
    let reply = who(&url, None).await;
    assert_eq!(reply.status(), 401);
    let metadata_url = format!("{url}/.well-known/oauth-protected-resource");
    assert_eq!(
        challenge(&reply),
        format!(r#"DPoP algs="ES256", resource_metadata="{metadata_url}""#)
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
    assert!(resource.dpop_bound_access_tokens_required);

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
    assert_eq!(server.dpop_signing_alg_values_supported, ["ES256"]);
}

#[tokio::test]
async fn a_live_token_passes_and_a_bad_one_gets_invalid_token() {
    let (service, url) = common::start(true, &[]).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();

    assert_eq!(
        who(&url, Some((&key, &pair.access_token))).await.status(),
        200
    );

    let reply = who(&url, Some((&key, "nope"))).await;
    assert_eq!(reply.status(), 401);
    assert!(challenge(&reply).starts_with(r#"DPoP error="invalid_token""#));
}

#[tokio::test]
async fn a_token_in_the_query_string_does_not_count() {
    let (service, url) = common::start(true, &[]).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let who_url = format!("{url}/v1/who");
    let reply = reqwest::Client::new()
        .post(format!("{who_url}?access_token={}", pair.access_token))
        .header(
            "dpop",
            key.proof("POST", &who_url, Some(&pair.access_token), common::now()),
        )
        .header("content-type", "application/json")
        .body(r#"{"me":"riff://mike@pangolin"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 401);
}

#[tokio::test]
async fn without_the_setting_no_token_is_needed() {
    let (_, url) = common::start(false, &[]).await;
    assert_eq!(who(&url, None).await.status(), 200);
}

#[tokio::test]
async fn the_token_endpoint_checks_the_resource() {
    let (service, url) = common::start(true, &[]).await;
    let key = Key::generate();
    let pair = service
        .tokens()
        .sign_in("mike", &key.thumbprint(), Instant::now())
        .unwrap();
    let form = |resource: &str| {
        format!(
            "grant_type=refresh_token&refresh_token={}&resource={resource}",
            pair.refresh_token
        )
    };

    let other = common::refresh(&url, &key, &form("https://evil.example.com")).await;
    assert_eq!(other.status(), 400);
    let error: TokenError = other.json().await.unwrap();
    assert_eq!(error.error, "invalid_target");

    // The refused request did not use the refresh token.
    let ours = common::refresh(&url, &key, &form(&url)).await;
    assert_eq!(ours.status(), 200);
}
