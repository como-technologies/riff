//! A server on a free port, and requests with DPoP proofs.

#![allow(dead_code)]

use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::routing::get;
use axum::{Json, Router};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use riff_core::wire::Discovery;
use serde_json::{Value, json};

use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::lease::Timing;
use riff_server::store::{Store, StoreError};

/// Sends a claim that the server refuses with the code `held`
/// (01M3WRD9JBQMNN96TXJH8EAJ3W). Gives the text of the refusal: it
/// names the holder.
pub async fn held(base: &str, claim: Value) -> String {
    let reply = client()
        .post(format!("{base}/v1/claim"))
        .json(&claim)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), 409, "a claim of a held item");
    assert_eq!(reply.headers()[riff_core::wire::REFUSED_HEADER], "held");
    reply.text().await.unwrap()
}

/// The HTTP client of each test. The build of a client blocks the
/// runtime for up to 250 ms under load: half of the [`LEASE`] serve
/// time. So the tests build one client, before the first server starts.
/// It keeps no idle connection, so a test never gets a connection of
/// the runtime of another test.
pub fn client() -> reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .default_headers(build_header())
            .build()
            .unwrap()
    });
    CLIENT.clone()
}

/// The build header of `riff`, so that each call of a test matches the
/// server.
pub fn build_header() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        riff_core::build::HEADER,
        riff_core::build::Build::this().to_string().parse().unwrap(),
    );
    headers
}

/// Starts a server. Its public URL is its real address.
pub async fn start(require_sign_in: bool, admins: &[&str]) -> (Service, String) {
    client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Service::new(Config {
        require_sign_in,
        admins: admins.iter().map(|a| a.to_string()).collect(),
        ..Config::new(&url)
    });
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// Short lease times, so that a test does not wait 15 seconds.
pub const LEASE: Timing = Timing {
    wait: Duration::from_millis(50),
    read_every: Duration::from_millis(50),
    valid_for: Duration::from_millis(500),
    exit_after: Duration::from_secs(1),
    renew_every: Duration::from_secs(30),
    ends_after: Duration::from_secs(90),
};

/// A short time between two writes of the token store, so that a test
/// does not wait a second for each write.
pub const SAVE_EVERY: Duration = Duration::from_millis(20);

/// Starts a server that loads its state from `store` and saves to it.
/// It uses the [`LEASE`] times and the [`SAVE_EVERY`] time.
pub async fn start_on(store: Arc<dyn Store>) -> (Service, String) {
    start_on_every(store, SAVE_EVERY).await
}

/// As [`start_on`], with `save_every` between two writes of the token
/// store.
pub async fn start_on_every(store: Arc<dyn Store>, save_every: Duration) -> (Service, String) {
    client();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease: LEASE,
        save_every,
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

/// Loads a server from `store` with the [`LEASE`] times, and does not
/// serve it.
pub async fn load_on(store: Arc<dyn Store>) -> Result<Service, StoreError> {
    let config = Config {
        lease: LEASE,
        save_every: SAVE_EVERY,
        ..Config::new("http://127.0.0.1:7878")
    };
    Service::load(config, store).await
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A POST with the `DPoP` scheme and a fresh proof from `key`.
pub fn post(url: &str, key: &Key, token: Option<&str>) -> reqwest::RequestBuilder {
    let mut request = client()
        .post(url)
        .header("dpop", key.proof("POST", url, token, now()));
    if let Some(token) = token {
        request = request.header("authorization", format!("DPoP {token}"));
    }
    request
}

/// How long a test tries a call again while the server does not serve.
pub const BUSY_LIMIT: Duration = Duration::from_secs(30);

/// A refresh at the token endpoint of `base`, with a proof from `key`.
/// As `riff` does, it tries again while the gate replies 503 (R132):
/// under load, the lease read of a test server can be older than the
/// 500 ms of [`LEASE`] (R139). Only the 503 of the gate has the header
/// `retry-after`, so a 503 of the token endpoint comes back at once.
pub async fn refresh(base: &str, key: &Key, form: &str) -> reqwest::Response {
    let started = std::time::Instant::now();
    loop {
        let reply = post(&format!("{base}/v1/token"), key, None)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .unwrap();
        let gate = reply.status() == 503 && reply.headers().contains_key("retry-after");
        if !gate || started.elapsed() > BUSY_LIMIT {
            return reply;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

const KEY: &str = include_str!("../../testdata/test-only-rsa-key.pem");
const JWKS: &str = include_str!("../../testdata/test-only-jwks.json");

/// A sign-in provider with only the two documents that the server
/// fetches: the discovery document and the JWKS. Returns its issuer.
pub async fn fake_provider() -> String {
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

/// An ID token of [`fake_provider`] for the client `riff-client`, with
/// a verified email. `domain` is the `hd` claim: `None` for a personal
/// account.
pub fn id_token(issuer: &str, email: &str, domain: Option<&str>) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test".into());
    let mut claims = json!({
        "iss": issuer,
        "aud": "riff-client",
        "exp": now() + 3600,
        "email": email,
        "email_verified": true,
    });
    if let Some(domain) = domain {
        claims["hd"] = json!(domain);
    }
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}
