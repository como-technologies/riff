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
//! - The sign-in goes to the keyring through [`crate::secrets`], as one
//!   secret for each server: [`secret_name`].
//! - Each token request carries a proof from the device key of the
//!   server ([`crate::device`]). The server binds the sign-in to that
//!   key (R18).
//!
//! [`access_token`] gives a live access token. It refreshes the pair
//! when the access token has less than [`REFRESH_MARGIN`] left.
//!
//! # Example
//!
//! ```
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! use riff::login::{self, SignIn};
//!
//! let server = "http://127.0.0.1:7878";
//! assert_eq!(login::user(server), None);
//! let sign_in = SignIn {
//!     user: "mike".into(),
//!     access_token: "a-1".into(),
//!     refresh_token: "r-1".into(),
//!     expires_at: 0,
//! };
//! login::store(server, &sign_in)?;
//! assert_eq!(login::user(server).as_deref(), Some("mike"));
//! assert!(login::logout(server)?);
//! assert_eq!(login::stored(server)?, None);
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use riff_core::wire::{
    Discovery, ID_TOKEN_TYPE, Revoked, SignInConfig, TOKEN_EXCHANGE, TokenRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use crate::api::Api;
use crate::{device, secrets};

/// [`access_token`] refreshes when less than this is left.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// How long `riff login` waits for the browser.
pub const BROWSER_WAIT: Duration = Duration::from_secs(5 * 60);

/// The sign-in of this device at one server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignIn {
    /// The user part of the session URI (R36).
    pub user: String,
    pub access_token: String,
    pub refresh_token: String,
    /// When the access token expires, in seconds since 1970.
    pub expires_at: u64,
}

/// The keyring name of the sign-in at `server`.
///
/// ```
/// assert_eq!(riff::login::secret_name("http://127.0.0.1:7878"), "sign-in http://127.0.0.1:7878");
/// ```
pub fn secret_name(server: &str) -> String {
    format!("sign-in {server}")
}

/// The sign-in at `server`, if there is one.
pub fn stored(server: &str) -> Result<Option<SignIn>> {
    secrets::get(&secret_name(server))?
        .map(|json| serde_json::from_str(&json).context("the stored sign-in is bad"))
        .transpose()
}

/// Keeps the sign-in at `server`.
pub fn store(server: &str, sign_in: &SignIn) -> Result<()> {
    secrets::set(&secret_name(server), &serde_json::to_string(sign_in)?)
}

/// The user of the sign-in at `server`. `None` when there is no
/// sign-in, or the keyring cannot be read.
pub fn user(server: &str) -> Option<String> {
    stored(server).ok().flatten().map(|s| s.user)
}

/// Removes the sign-in at `server` from this device. Returns false when
/// there was none.
pub fn logout(server: &str) -> Result<bool> {
    let had = stored(server)?.is_some();
    secrets::delete(&secret_name(server))?;
    Ok(had)
}

/// Ends each sign-in of `user` on each device (R101). The default user
/// is the caller. Removes the sign-in from this device too when it ends.
pub async fn logout_all(api: &Api, user: Option<&str>) -> Result<Revoked> {
    let token = access_token(api).await?;
    let key = device::key(api.base())?;
    let done = api.clone().with_token(&token, key).revoke(user).await?;
    if stored(api.base())?.is_some_and(|s| s.user == done.user) {
        secrets::delete(&secret_name(api.base()))?;
    }
    Ok(done)
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
    let pair = api
        .token(
            &TokenRequest {
                grant_type: TOKEN_EXCHANGE.into(),
                subject_token: Some(id_token),
                subject_token_type: Some(ID_TOKEN_TYPE.into()),
                ..TokenRequest::default()
            },
            &device::key(api.base())?,
        )
        .await?;
    let sign_in = SignIn {
        expires_at: now() + pair.expires_in,
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
    };
    store(api.base(), &sign_in)?;
    Ok(sign_in)
}

/// A live access token for the server of `api`. It refreshes the pair
/// when needed, and keeps the new pair.
pub async fn access_token(api: &Api) -> Result<String> {
    let Some(sign_in) = stored(api.base())? else {
        bail!("no sign-in for {}: run riff login", api.base());
    };
    if now() + REFRESH_MARGIN.as_secs() < sign_in.expires_at {
        return Ok(sign_in.access_token);
    }
    let pair = api
        .token(
            &TokenRequest {
                grant_type: "refresh_token".into(),
                refresh_token: Some(sign_in.refresh_token),
                ..TokenRequest::default()
            },
            &device::key(api.base())?,
        )
        .await
        .context("the sign-in ended: run riff login")?;
    let fresh = SignIn {
        expires_at: now() + pair.expires_in,
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
    };
    store(api.base(), &fresh)?;
    Ok(fresh.access_token)
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
/// Other requests, for example for a favicon, get 404.
async fn receive_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (stream, _) = listener.accept().await?;
        let mut stream = BufReader::new(stream);
        let mut line = String::new();
        stream.read_line(&mut line).await?;
        let target = line.split_whitespace().nth(1).unwrap_or("/");
        let url = Url::parse("http://127.0.0.1")?.join(target)?;
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let outcome = match (query.get("code"), query.get("error")) {
            (_, Some(error)) => Err(anyhow::anyhow!("the sign-in failed: {error}")),
            (Some(_), _) if query.get("state").map(String::as_str) != Some(state) => {
                Err(anyhow::anyhow!("the sign-in came back with a wrong state"))
            }
            (Some(code), _) => Ok(code.clone()),
            (None, None) => {
                respond(stream.get_mut(), "404 Not Found", "").await;
                continue;
            }
        };
        let page = match &outcome {
            Ok(_) => "riff: you are signed in. You can close this tab.",
            Err(_) => "riff: the sign-in failed. See the terminal.",
        };
        respond(stream.get_mut(), "200 OK", page).await;
        return outcome;
    }
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
    let response = http
        .post(&discovery.token_endpoint)
        .form(&form)
        .send()
        .await
        .context("cannot reach the sign-in provider")?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        bail!("the sign-in provider refused the code ({status}): {text}");
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
    async fn the_loopback_port_takes_the_code_and_checks_the_state() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let get = |path: &str| {
            let url = format!("{base}{path}");
            tokio::spawn(async move { reqwest::get(url).await.map(|r| r.status().as_u16()) })
        };
        let favicon = get("/favicon.ico");
        let wrong = get("/?code=c1&state=other");
        let first = receive_code(&listener, "s1").await;
        let second = {
            let good = get("/?code=c2&state=s1");
            let code = receive_code(&listener, "s1").await;
            assert_eq!(good.await.unwrap().unwrap(), 200);
            code
        };
        // The favicon request may come before or after the wrong state.
        assert_eq!(favicon.await.unwrap().unwrap(), 404);
        assert_eq!(wrong.await.unwrap().unwrap(), 200);
        assert!(first.unwrap_err().to_string().contains("wrong state"));
        assert_eq!(second.unwrap(), "c2");
    }

    #[tokio::test]
    async fn a_provider_error_ends_the_sign_in() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/?error=access_denied",
            listener.local_addr().unwrap()
        );
        tokio::spawn(reqwest::get(url));
        let error = receive_code(&listener, "s").await.unwrap_err();
        assert!(error.to_string().contains("access_denied"));
    }
}
