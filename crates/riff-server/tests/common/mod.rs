//! A server on a free port, and requests with DPoP proofs.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::lease::Timing;
use riff_server::store::Store;

/// Starts a server. Its public URL is its real address.
pub async fn start(require_sign_in: bool, admins: &[&str]) -> (Service, String) {
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
};

/// Starts a server that loads its state from `store` and saves to it.
/// It uses the [`LEASE`] times.
pub async fn start_on(store: Arc<dyn Store>) -> (Service, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = Config {
        lease: LEASE,
        ..Config::new(&url)
    };
    let service = Service::load(config, store).await.unwrap();
    let router = service.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (service, url)
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A POST with the `DPoP` scheme and a fresh proof from `key`.
pub fn post(url: &str, key: &Key, token: Option<&str>) -> reqwest::RequestBuilder {
    let mut request = reqwest::Client::new()
        .post(url)
        .header("dpop", key.proof("POST", url, token, now()));
    if let Some(token) = token {
        request = request.header("authorization", format!("DPoP {token}"));
    }
    request
}

/// A refresh at the token endpoint of `base`, with a proof from `key`.
pub async fn refresh(base: &str, key: &Key, form: &str) -> reqwest::Response {
    post(&format!("{base}/v1/token"), key, None)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(form.to_owned())
        .send()
        .await
        .unwrap()
}
