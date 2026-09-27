//! A server on a free port, and requests with DPoP proofs.

#![allow(dead_code)]

use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use riff_core::dpop::Key;
use riff_server::Service;
use riff_server::auth::Config;
use riff_server::lease::Timing;
use riff_server::store::Store;

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
};

/// Starts a server that loads its state from `store` and saves to it.
/// It uses the [`LEASE`] times.
pub async fn start_on(store: Arc<dyn Store>) -> (Service, String) {
    client();
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
    let mut request = client()
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
