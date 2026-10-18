//! The sign-in of this device: the person token pair, its refresh and
//! the token exchange of a session (R90).
//!
//! `riff login` gets an ID token from the sign-in provider in the
//! browser. That step is in the crate `riff`. Then [`sign_in_with`]
//! swaps the ID token for a riff sign-in at the server, and keeps it.
//!
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
//! refresh token would end the sign-in. The lock file is in the local
//! directory of riff, not in `TMPDIR`, because each worker has a
//! `TMPDIR` of its own: see [`lock_path`].
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
//! (01M52JYGWD3ZJKNYE3HG5BJYTP). The session token stays in the memory
//! of the process (see [`crate::api::Api::signed_in`]); it never goes
//! to the keyring.
//!
//! # Example
//!
//! ```standalone_crate
//! # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
//! use riff_client::login::{self, SignIn};
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

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, ID_TOKEN_TYPE, Revoked, SignInConfig, TOKEN_EXCHANGE, TokenReply,
    TokenRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::{Api, TokenRefused};
use crate::{device, files, secrets};

/// [`access_token`] refreshes when less than this is left.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// The error of a refresh that the server refused: the sign-in ended
/// (01M3MX4TSEH18FSNQ28GEH2GFJ).
pub const ENDED: &str = "the sign-in ended: run riff login";

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
    /// use riff_client::login::SignIn;
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
/// assert_eq!(riff_client::login::secret_name("http://127.0.0.1:7878"), "sign-in http://127.0.0.1:7878");
/// ```
pub fn secret_name(server: &str) -> String {
    format!("sign-in {server}")
}

/// The sign-in at `server`, if there is one. A process reads it from
/// the keyring one time ([`secrets::kept`], 01M54510PS9FDKQKJB0RAH3XG3).
pub fn stored(server: &str) -> Result<Option<SignIn>> {
    parse(secrets::kept(&secret_name(server))?)
}

/// The sign-in at `server` from a new read of the keyring: under the
/// refresh lock, another process can have changed it.
fn fresh(server: &str) -> Result<Option<SignIn>> {
    parse(secrets::get(&secret_name(server))?)
}

fn parse(json: Option<String>) -> Result<Option<SignIn>> {
    json.map(|json| serde_json::from_str(&json).context("the stored sign-in is bad"))
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

/// Swaps the ID token of the provider for a riff sign-in, and keeps it.
pub async fn sign_in_with(api: &Api, config: &SignInConfig, id_token: String) -> Result<SignIn> {
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
    if let Some(live) = live(api.base(), stored(api.base())?)? {
        return Ok(live.access_token);
    }
    let _lock = refresh_lock(api.base()).await?;
    // Another process may have refreshed while this one waited.
    let sign_in = fresh(api.base())?;
    if let Some(live) = live(api.base(), sign_in.clone())? {
        return Ok(live.access_token);
    }
    let Some(sign_in) = sign_in else {
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
            if fresh(api.base())?.as_ref() == Some(&sign_in) {
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
    if let Some(sign_in) = fresh(server)?
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

/// The sign-in `sign_in` at `server` when its access token is live.
/// `Err` when there is no sign-in.
fn live(server: &str, sign_in: Option<SignIn>) -> Result<Option<SignIn>> {
    let Some(sign_in) = sign_in else {
        bail!("no sign-in for {server}: run riff login");
    };
    Ok((now() + REFRESH_MARGIN.as_secs() < sign_in.expires_at).then_some(sign_in))
}

/// The lock file for refreshes of the pair at `server` (R107): one for
/// each server, in the local directory of the platform
/// ([`Platform::local_dir`]), never in `TMPDIR`
/// (01M52JMEKD4P2ZT659CBNPN9CC), except in a test.
///
/// `riff workers run` gives each worker a `TMPDIR` of its own. A lock
/// in `TMPDIR` is then one lock for each worker, and two workers send
/// one refresh token at the same time. The server keeps only the pair
/// of one of them, and the next refresh with the other pair ends the
/// sign-in. The local directory is the same in each `riff` process of
/// the user on the machine, so it holds one lock. It is per user, so
/// the name holds only the server. An old lock file in `TMPDIR` stays
/// there: no `riff` opens it again. `Err` when the platform has no
/// local directory: `riff` with no `HOME`.
///
/// A process that set the store of its secrets itself, a test, keeps
/// its sign-in in that store, not in the OS keyring
/// ([`secrets::own_store`]). Its lock file stays in `TMPDIR`, so that a
/// test never touches the local files of the machine
/// (01M3MY2KWKBJCQ0BCNC6533RBW).
///
/// [`Platform::local_dir`]: crate::platform::Platform::local_dir
///
/// ```standalone_crate
/// use std::path::PathBuf;
/// use riff_client::{login, platform::{self, Platform}};
///
/// struct Local;
///
/// impl Platform for Local {
///     fn local_dir(&self) -> Option<PathBuf> {
///         Some("/run/user/1000/riff".into())
///     }
/// }
///
/// assert!(login::lock_path("http://a").is_err());
/// platform::set(Box::new(Local));
/// let a = login::lock_path("http://a")?;
/// assert_ne!(a, login::lock_path("http://b")?);
/// assert_eq!(a.parent(), Some("/run/user/1000/riff".as_ref()));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn lock_path(server: &str) -> Result<PathBuf> {
    let dir = if secrets::own_store() {
        std::env::temp_dir()
    } else {
        crate::platform::get()
            .local_dir()
            .context("no HOME: riff has no local dir")?
    };
    let hash = Sha256::digest(server.as_bytes());
    let name = URL_SAFE_NO_PAD.encode(&hash[..12]);
    Ok(dir.join(format!("riff-sign-in-{name}.lock")))
}

/// Waits for the refresh lock of `server`. The lock ends when the file
/// closes.
async fn refresh_lock(server: &str) -> Result<std::fs::File> {
    let path = lock_path(server)?;
    // The temp folder of a test is not the local folder: it keeps its
    // mode.
    if let Some(dir) = path.parent() {
        let made = if secrets::own_store() {
            std::fs::create_dir_all(dir)
        } else {
            files::make_dir(dir)
        };
        made.with_context(|| format!("cannot make {}", dir.display()))?;
    }
    let file = files::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    files::private(&file).with_context(|| format!("cannot set the mode of {}", path.display()))?;
    tokio::task::spawn_blocking(move || file.lock().map(|()| file))
        .await?
        .with_context(|| format!("cannot lock {}", path.display()))
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
}
