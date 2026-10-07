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
//! [`access_token`] gives a live person access token. It refreshes the
//! pair when the access token has less than [`REFRESH_MARGIN`] left.
//! Only one `riff` process at a time refreshes the pair of one server:
//! a lock file makes the others wait (R107). Two processes that use one
//! refresh token would end the sign-in.
//!
//! Only a refusal of the grant says that the sign-in ended
//! ([`ENDED`], 01M3MX4TSEH18FSNQ28GEH2GFJ). Each other error of a
//! refresh keeps its own text: for example a server that is down, or
//! another version.
//!
//! A refused refresh is not sent again (01M3W947QF6PFBWR28ZVXCVQHG). The
//! client keeps the sign-in with no refresh token: [`SignIn::ended`].
//! From then on, [`access_token`] gives [`ENDED`] with no call to the
//! server, in each process of the machine, until `riff login` keeps a
//! new sign-in.
//!
//! ```mermaid
//! flowchart TD
//!     A[access_token] --> L{access token live?}
//!     L -- yes --> T[give it]
//!     L -- no --> E{kept sign-in ended?}
//!     E -- yes --> X[ENDED, no call]
//!     E -- no --> R[POST /v1/token refresh]
//!     R -- pair --> K[keep the new pair] --> T
//!     R -- invalid_grant --> M[keep the sign-in as ended] --> X
//!     R -- other error --> O[the error, the sign-in stays]
//! ```
//!
//! [`session_token`] swaps the person access token for a session
//! access token (R19). It has no refresh token
//! (01M3WFVAB44T8EP4QZD4KS7DRF). The session token stays in the memory
//! of the process (see [`crate::api::Api::signed_in`]); it never goes
//! to the keyring.
//!
//! # Example
//!
//! ```
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! use riff::login::{self, SignIn};
//!
//! let server = "http://127.0.0.1:7878";
//! assert_eq!(login::user(server)?, None);
//! let sign_in = SignIn {
//!     user: "mike".into(),
//!     access_token: "a-1".into(),
//!     refresh_token: "r-1".into(),
//!     expires_at: 0,
//!     riff_id: None,
//! };
//! login::store(server, &sign_in)?;
//! assert_eq!(login::user(server)?.as_deref(), Some("mike"));
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
    ACCESS_TOKEN_TYPE, Discovery, ID_TOKEN_TYPE, Revoked, SignInConfig, TOKEN_EXCHANGE, TokenReply,
    TokenRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use crate::api::{Api, TokenRefused};
use crate::{device, secrets};

/// [`access_token`] refreshes when less than this is left.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// The error of a refresh that the server refused: the sign-in ended
/// (01M3MX4TSEH18FSNQ28GEH2GFJ).
pub const ENDED: &str = "the sign-in ended: run riff login";

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
    /// The ID of the riff of the sign-in (01M3JNVBRS35B3CD67367JF7SJ).
    /// `None` for a sign-in from before the riff ID: it counts as old.
    #[serde(default)]
    pub riff_id: Option<String>,
}

impl SignIn {
    /// True when the server refused the refresh token of this sign-in:
    /// the sign-in ended (01M3W947QF6PFBWR28ZVXCVQHG). It keeps its user,
    /// and has no refresh token.
    ///
    /// ```
    /// use riff::login::SignIn;
    ///
    /// let sign_in = SignIn {
    ///     user: "mike".into(),
    ///     access_token: "a-1".into(),
    ///     refresh_token: "r-1".into(),
    ///     expires_at: 0,
    ///     riff_id: None,
    /// };
    /// assert!(!sign_in.ended());
    /// let ended = sign_in.end();
    /// assert!(ended.ended());
    /// assert_eq!(ended.user, "mike");
    /// ```
    pub fn ended(&self) -> bool {
        self.refresh_token.is_empty()
    }

    /// This sign-in, ended: with no token that the server takes.
    pub fn end(self) -> SignIn {
        SignIn {
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: 0,
            ..self
        }
    }
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

/// The user of the sign-in at `server`, or `None` when there is no
/// sign-in. A keyring error is an error (R157).
pub fn user(server: &str) -> Result<Option<String>> {
    Ok(stored(server)?.map(|s| s.user))
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
    let done = api.clone().signed_in(None)?.revoke(user).await?;
    if stored(api.base())?.is_some_and(|s| s.user == done.user) {
        secrets::delete(&secret_name(api.base()))?;
    }
    Ok(done)
}

/// Signs in at the server of `api` when the server has sign-in and this
/// device has no sign-in there (01M4BYH84X7B2D9EFYGP11GP8Y). A sign-in of
/// a riff that is gone does not count: it is removed first
/// (01M3JNVBRS35B3CD67367JF7SJ). Returns the new sign-in, or `None` when
/// the riff has no sign-in or this device has one. A sign-in that ended
/// ([`SignIn::ended`]) does not count. `riff` runs it.
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

/// Swaps the ID token of the provider for a riff sign-in, and keeps it.
async fn sign_in_with(api: &Api, config: &SignInConfig, id_token: String) -> Result<SignIn> {
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
        riff_id: Some(config.riff_id.clone()),
    };
    store(api.base(), &sign_in)?;
    Ok(sign_in)
}

/// A live person access token for the server of `api`. It refreshes
/// the pair when needed, and keeps the new pair. When the server
/// refuses the refresh token, it keeps the sign-in as ended, and each
/// later call gives [`ENDED`] with no call to the server
/// (01M3W947QF6PFBWR28ZVXCVQHG).
pub async fn access_token(api: &Api) -> Result<String> {
    if let Some(live) = live(api.base())? {
        return Ok(live.access_token);
    }
    let _lock = refresh_lock(api.base()).await?;
    // Another process may have refreshed while this one waited.
    if let Some(live) = live(api.base())? {
        return Ok(live.access_token);
    }
    let Some(sign_in) = stored(api.base())? else {
        bail!("no sign-in for {}: run riff login", api.base());
    };
    if sign_in.ended() {
        bail!(ENDED);
    }
    let refreshed = api
        .token(
            &TokenRequest {
                grant_type: "refresh_token".into(),
                refresh_token: Some(sign_in.refresh_token.clone()),
                ..TokenRequest::default()
            },
            &device::key(api.base())?,
        )
        .await;
    let pair = match refreshed {
        Ok(pair) => pair,
        Err(e)
            if e.downcast_ref::<TokenRefused>()
                .is_some_and(TokenRefused::ended) =>
        {
            // `riff login` takes no lock: keep a sign-in that came
            // during the call.
            if stored(api.base())?.as_ref() == Some(&sign_in) {
                store(api.base(), &sign_in.end())?;
            }
            return Err(e.context(ENDED));
        }
        Err(e) => return Err(e),
    };
    let fresh = SignIn {
        expires_at: now() + pair.expires_in,
        user: pair.user,
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        riff_id: sign_in.riff_id,
    };
    store(api.base(), &fresh)?;
    Ok(fresh.access_token)
}

/// Drops the person access token `token` at `server` when the keyring
/// still holds it, so that the next [`access_token`] refreshes the pair
/// (01M3MX4VCEBTY0DN4JMF624WYE). The server did not take the token.
pub async fn forget(server: &str, token: &str) -> Result<()> {
    let _lock = refresh_lock(server).await?;
    if let Some(sign_in) = stored(server)?
        && sign_in.access_token == token
    {
        let old = SignIn {
            expires_at: 0,
            ..sign_in
        };
        store(server, &old)?;
    }
    Ok(())
}

/// A new session access token for `session`, from the person access
/// token of the server of `api` (R19). It works only for that session,
/// and its reply has no refresh token.
/// When the server does not take the person access token, it refreshes
/// the person pair once and asks again (01M3MX4VCEBTY0DN4JMF624WYE).
pub async fn session_token(api: &Api, session: &str) -> Result<TokenReply> {
    let person = access_token(api).await?;
    match exchange(api, session, &person).await {
        Err(e)
            if e.downcast_ref::<TokenRefused>()
                .is_some_and(TokenRefused::ended) =>
        {
            forget(api.base(), &person).await?;
            let person = access_token(api).await?;
            exchange(api, session, &person).await
        }
        reply => reply,
    }
}

/// Swaps the person access token `person` for a session access token.
async fn exchange(api: &Api, session: &str, person: &str) -> Result<TokenReply> {
    api.token(
        &TokenRequest {
            grant_type: TOKEN_EXCHANGE.into(),
            subject_token: Some(person.to_owned()),
            subject_token_type: Some(ACCESS_TOKEN_TYPE.into()),
            session: Some(session.to_owned()),
            ..TokenRequest::default()
        },
        &device::key(api.base())?,
    )
    .await
}

/// The sign-in at `server` when its access token is live. `Err` when
/// there is no sign-in.
fn live(server: &str) -> Result<Option<SignIn>> {
    let Some(sign_in) = stored(server)? else {
        bail!("no sign-in for {server}: run riff login");
    };
    Ok((now() + REFRESH_MARGIN.as_secs() < sign_in.expires_at).then_some(sign_in))
}

/// The lock file for refreshes of the pair at `server`, one for each OS
/// user and server (R107).
///
/// ```
/// let a = riff::login::lock_path("http://a");
/// assert_ne!(a, riff::login::lock_path("http://b"));
/// assert!(a.starts_with(std::env::temp_dir()));
/// ```
pub fn lock_path(server: &str) -> std::path::PathBuf {
    let owner = std::env::var("USER").unwrap_or_default();
    let hash = Sha256::digest(format!("{owner} {server}").as_bytes());
    let name = URL_SAFE_NO_PAD.encode(&hash[..12]);
    std::env::temp_dir().join(format!("riff-sign-in-{name}.lock"))
}

/// Waits for the refresh lock of `server`. The lock ends when the file
/// closes.
async fn refresh_lock(server: &str) -> Result<std::fs::File> {
    let path = lock_path(server);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    tokio::task::spawn_blocking(move || file.lock().map(|()| file))
        .await?
        .with_context(|| format!("cannot lock {}", path.display()))
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

    #[test]
    fn an_ended_sign_in_keeps_its_user_and_its_riff_and_no_token() {
        let sign_in = SignIn {
            user: "ada".into(),
            access_token: "a-1".into(),
            refresh_token: "r-1".into(),
            expires_at: now() + 600,
            riff_id: Some("riff-1".into()),
        };
        assert!(!sign_in.ended());
        let ended = sign_in.end();
        assert!(ended.ended());
        assert_eq!(ended.user, "ada");
        assert_eq!(ended.riff_id.as_deref(), Some("riff-1"));
        assert_eq!((ended.access_token.as_str(), ended.expires_at), ("", 0));
        let kept: SignIn = serde_json::from_str(&serde_json::to_string(&ended).unwrap()).unwrap();
        assert!(kept.ended());
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
