//! `riff login` and `riff logout`: the sign-in of this device (R90).
//!
//! # Flow
//!
//! ```mermaid
//! sequenceDiagram
//!     participant B as Browser
//!     participant R as riff login
//!     participant P as Provider (Google)
//!     participant S as riff-server
//!     R->>S: GET /v1/sign-in
//!     R->>P: GET discovery document
//!     R->>B: open authorize URL
//!     B->>P: sign in
//!     P->>B: redirect to 127.0.0.1:PORT/?code&state
//!     B->>R: code
//!     R->>P: code + PKCE verifier
//!     P-->>R: ID token
//!     R->>S: token exchange
//!     S-->>R: riff tokens + user
//! ```
//!
//! - The code comes to a loopback port that `riff login` opens for one
//!   sign-in. The port is random.
//! - PKCE uses S256. `state` must come back unchanged.
//! - The server checks the ID token (see `riff_server::oidc`). The
//!   client does not.
//! - This module has the browser step (01M56S4M2NMATA3XZAMPSRAW8X). The
//!   token exchange, the keep of the sign-in, its refresh and the
//!   session tokens are in [`riff_client::login`].

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use riff_client::api::Api;
use riff_client::login::{SignIn, sign_in_with, stored};
use riff_core::wire::{Discovery, SignInConfig};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// How long `riff login` waits for the browser.
pub const BROWSER_WAIT: Duration = Duration::from_secs(5 * 60);

/// Signs in at the server of `api` when the server has sign-in and this
/// device has no sign-in there (01M3JZN1ZZED3FXQEFNJ4KVCN5). A sign-in of
/// a riff that is gone does not count: it is removed first
/// (01M3JNVBRS35B3CD67367JF7SJ). Returns the new sign-in, or `None` when
/// the riff has no sign-in or this device has one. A sign-in that ended
/// ([`SignIn::ended`]) does not count. `riff connect claude` runs it.
pub async fn ensure(api: &Api, open: impl FnOnce(&str)) -> Result<Option<SignIn>> {
    if !api.has_sign_in().await? {
        return Ok(None);
    }
    // An error here says that the old sign-in is gone: sign in again.
    let _ = api.check_riff().await;
    if stored(api.base())?.is_some_and(|s| !s.ended()) {
        return Ok(None);
    }
    login(api, open).await.map(Some)
}

/// Signs in at the server of `api`. `open` shows the authorize URL to
/// the person, for example in the browser. Keeps the sign-in and
/// returns it.
pub async fn login(api: &Api, open: impl FnOnce(&str)) -> Result<SignIn> {
    let config = api.sign_in_config().await?;
    let http = reqwest::Client::new();
    let discovery: Discovery = http
        .get(Discovery::url(&config.issuer))
        .send()
        .await
        .context("cannot reach the sign-in provider")?
        .error_for_status()?
        .json()
        .await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}/", listener.local_addr()?.port());
    let verifier = random();
    let state = random();
    let url = authorize_url(
        &discovery.authorization_endpoint,
        &config.client_id,
        &redirect,
        &challenge(&verifier),
        &state,
    )?;
    open(url.as_str());
    let code = tokio::time::timeout(BROWSER_WAIT, receive_code(&listener, &state))
        .await
        .context("no sign-in came back from the browser")??;
    let id_token = redeem(&http, &discovery, &config, &code, &redirect, &verifier).await?;
    sign_in_with(api, &config, id_token).await
}

/// Signs in at the server of `api` with a refresh token of the
/// provider, with no browser: the smoke test of a riff in the cloud
/// signs in as a test account this way (01M496JTHN19BZ7YN94993R35X).
/// The provider gives an ID token for it. Keeps the sign-in and returns
/// it.
pub async fn login_with_refresh_token(api: &Api, refresh_token: &str) -> Result<SignIn> {
    let config = api.sign_in_config().await?;
    let http = reqwest::Client::new();
    let discovery: Discovery = http
        .get(Discovery::url(&config.issuer))
        .send()
        .await
        .context("cannot reach the sign-in provider")?
        .error_for_status()?
        .json()
        .await?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &config.client_id),
    ];
    if let Some(secret) = &config.client_secret {
        form.push(("client_secret", secret));
    }
    let id_token = provider_id_token(&http, &discovery, &form, "refresh token").await?;
    sign_in_with(api, &config, id_token).await
}

/// The URL that starts the sign-in in the browser.
///
/// ```
/// let url = riff::login::authorize_url(
///     "https://accounts.google.com/o/oauth2/v2/auth", "riff", "http://127.0.0.1:5000/",
///     "abc", "xyz",
/// ).unwrap();
/// let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
/// assert_eq!(query["response_type"], "code");
/// assert_eq!(query["scope"], "openid email");
/// assert_eq!(query["code_challenge_method"], "S256");
/// assert_eq!(query["redirect_uri"], "http://127.0.0.1:5000/");
/// ```
pub fn authorize_url(
    endpoint: &str,
    client_id: &str,
    redirect: &str,
    challenge: &str,
    state: &str,
) -> Result<Url> {
    let mut url = Url::parse(endpoint).context("the authorization endpoint is not a URL")?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", "openid email")
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url)
}

/// The PKCE S256 challenge of `verifier` (RFC 7636).
///
/// ```
/// // The example from RFC 7636, appendix B.
/// assert_eq!(
///     riff::login::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
///     "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
/// );
/// ```
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Serves the loopback port until the redirect with the code comes.
/// Only a request with the right `state` ends the wait. Other requests,
/// for example for a favicon or from another local process, get 404
/// (R112).
async fn receive_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (stream, _) = listener.accept().await?;
        let mut stream = BufReader::new(stream);
        let mut line = String::new();
        if (&mut stream)
            .take(MAX_REQUEST_LINE)
            .read_line(&mut line)
            .await
            .is_err()
        {
            continue;
        }
        let target = line.split_whitespace().nth(1).unwrap_or("/");
        let Ok(url) = Url::parse("http://127.0.0.1")?.join(target) else {
            respond(stream.get_mut(), "404 Not Found", "").await;
            continue;
        };
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        if query.get("state").map(String::as_str) != Some(state) {
            respond(stream.get_mut(), "404 Not Found", "").await;
            continue;
        }
        let outcome = match (query.get("code"), query.get("error")) {
            (_, Some(error)) => Err(anyhow::anyhow!("the sign-in failed: {}", printable(error))),
            (Some(code), None) => Ok(code.clone()),
            (None, None) => Err(anyhow::anyhow!("the sign-in came back with no code")),
        };
        let page = match &outcome {
            Ok(_) => "riff: you are signed in. You can close this tab.",
            Err(_) => "riff: the sign-in failed. See the terminal.",
        };
        respond(stream.get_mut(), "200 OK", page).await;
        return outcome;
    }
}

/// The longest request line the loopback port reads, in bytes.
const MAX_REQUEST_LINE: u64 = 8192;

/// `text` with control characters removed, cut to 200 characters, so
/// that it is safe to print on a terminal.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}

async fn respond(stream: &mut tokio::net::TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    // The browser may be gone. The code is what counts.
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[derive(Deserialize)]
struct ProviderTokens {
    id_token: String,
}

/// Swaps the code for an ID token at the provider.
async fn redeem(
    http: &reqwest::Client,
    discovery: &Discovery,
    config: &SignInConfig,
    code: &str,
    redirect: &str,
    verifier: &str,
) -> Result<String> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", &config.client_id),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = &config.client_secret {
        form.push(("client_secret", secret));
    }
    provider_id_token(http, discovery, &form, "code").await
}

/// Sends `form` to the token endpoint of the provider, and returns the
/// ID token of the reply. `what` names the grant in the error.
async fn provider_id_token(
    http: &reqwest::Client,
    discovery: &Discovery,
    form: &[(&str, &str)],
    what: &str,
) -> Result<String> {
    let response = http
        .post(&discovery.token_endpoint)
        .form(form)
        .send()
        .await
        .context("cannot reach the sign-in provider")?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        bail!("the sign-in provider refused the {what} ({status}): {text}");
    }
    let tokens: ProviderTokens = response.json().await?;
    Ok(tokens.id_token)
}

/// 32 random bytes in URL-safe base64: a PKCE verifier or a state.
fn random() -> String {
    let mut bytes = [0u8; 32];
    // The OS random source fails only when the OS is broken.
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifiers_are_random_and_long_enough() {
        let a = random();
        assert_ne!(a, random());
        // RFC 7636: 43 to 128 characters.
        assert_eq!(a.len(), 43);
    }

    #[tokio::test]
    async fn the_loopback_port_waits_for_the_right_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let get = |path: &str| {
            let url = format!("{base}{path}");
            tokio::spawn(async move { reqwest::get(url).await.map(|r| r.status().as_u16()) })
        };
        // Requests from another local process do not end the wait.
        for path in ["/favicon.ico", "/?code=c1&state=other", "/?error=x"] {
            let other = get(path);
            let good = async {
                assert_eq!(other.await.unwrap().unwrap(), 404, "{path}");
                get("/?code=c2&state=s1").await.unwrap().unwrap()
            };
            let (code, good) = tokio::join!(receive_code(&listener, "s1"), good);
            assert_eq!(good, 200);
            assert_eq!(code.unwrap(), "c2");
        }
    }

    #[tokio::test]
    async fn a_long_request_line_is_ignored() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let long = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let _ = stream.write_all(&vec![b'a'; 100_000]).await;
        });
        let good = format!("http://{addr}/?code=c&state=s");
        let good = async {
            long.await.unwrap();
            reqwest::get(good).await.unwrap().status().as_u16()
        };
        let (code, good) = tokio::join!(receive_code(&listener, "s"), good);
        assert_eq!(good, 200);
        assert_eq!(code.unwrap(), "c");
    }

    #[tokio::test]
    async fn a_provider_error_ends_the_sign_in() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/?error=access%1B%5B31mdenied&state=s",
            listener.local_addr().unwrap()
        );
        tokio::spawn(reqwest::get(url));
        let error = receive_code(&listener, "s").await.unwrap_err();
        assert_eq!(error.to_string(), "the sign-in failed: access[31mdenied");
    }
}
