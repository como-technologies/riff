//! The secrets of a session come in its environment, not from the
//! keyring of the person (#611).
//!
//! # Design
//!
//! No process of a session reads the keyring of the person
//! (01M4CVXJ7ZDAVRKJ8Y59R3KPDV). The wrapper of the session (`riff workers run`
//! and `riff workers lead`) runs outside the sandbox. Before it starts
//! `claude`, it reads the sign-in of the person from the keyring and
//! makes the secrets of the session ([`Secrets::make`]):
//!
//! | Variable | What it holds |
//! |---|---|
//! | [`KEY_VAR`] | A new session key: a P-256 key of this session only |
//! | [`GRANT_VAR`] | A session grant of riff-server: it acts only as this session, and only with the session key |
//! | `RIFF_USER` | The user of the sign-in, so that no process of the session asks the keyring |
//! | [`CLAUDE_TOKEN_VAR`] | The Claude plan token of the person ([`claude_token`]), when the person keeps one |
//!
//! The forge token of the role comes in the token files of the session
//! ([`crate::forge`]).
//!
//! The grant does not rotate. So each process of the session, `riff
//! mcp`, each hook and each `riff` command, swaps the grant for a
//! session access token of its own, with a proof of the session key
//! ([`crate::api::Api::signed_in`]). The session key signs the posts of
//! the session. riff-server lists it with the keys of the person, so
//! the readers verify the posts (01M4CVXJ3GCEB7B4632J7DD84A).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant W as riff workers run (outside the sandbox)
//!     participant K as keyring of the person
//!     participant S as riff-server
//!     participant C as claude, riff mcp, hooks (in the sandbox)
//!     W->>K: the sign-in and the device key
//!     W->>W: a new session key
//!     W->>S: POST /v1/token: person token, device proof, session, session-key proof
//!     S-->>W: the session grant
//!     W->>C: start with RIFF_SESSION_KEY, RIFF_SESSION_GRANT, RIFF_USER
//!     C->>S: POST /v1/token: the grant, session-key proof
//!     S-->>C: a session access token
//!     C->>S: calls and signed posts with the session key
//! ```
//!
//! With a grant in the environment, riff never opens the keyring
//! ([`in_session`], [`crate::secrets`]). The [`Debug`] of [`Secrets`]
//! names no secret, and no error text holds one (01M4CVXJA9WAN5M1RKNGETS8AY).
//!
//! # Example
//!
//! ```
//! use riff::grant::{Secrets, GRANT_VAR, KEY_VAR};
//! use riff_core::dpop::Key;
//!
//! let key = Key::generate();
//! let secrets = Secrets::new(key, "g7.secret".into(), "mike".into(), Some("sk-ant-oat01-x".into()));
//! let env = secrets.env();
//! assert!(env.iter().any(|(k, v)| *k == GRANT_VAR && v == "g7.secret"));
//! assert!(env.iter().any(|(k, _)| *k == KEY_VAR));
//! assert!(env.iter().any(|(k, v)| *k == "RIFF_USER" && v == "mike"));
//! let shown = format!("{secrets:?}");
//! assert!(!shown.contains("g7.secret") && !shown.contains("sk-ant"), "{shown}");
//! ```

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use riff_core::dpop::Key;
use riff_core::wire::{
    ACCESS_TOKEN_TYPE, GRANT_TOKEN_TYPE, TOKEN_EXCHANGE, TokenReply, TokenRequest,
};

use crate::api::{Api, TokenRefused};
use crate::{device, login, secrets};

/// The variable of the session key.
pub const KEY_VAR: &str = "RIFF_SESSION_KEY";

/// The variable of the session grant.
pub const GRANT_VAR: &str = "RIFF_SESSION_GRANT";

/// The variable of the user of a session.
pub const USER_VAR: &str = "RIFF_USER";

/// The variable that gives Claude Code the plan token of the person
/// (01M4CVXJCGRC59VVZEHBFA9MPG).
pub const CLAUDE_TOKEN_VAR: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// The keyring name of the Claude plan token of the person. One token
/// for each machine, for each server.
pub const CLAUDE_TOKEN_SECRET: &str = "claude-oauth-token";

/// The text of a keyring call in a session (01M4CVXJ7ZDAVRKJ8Y59R3KPDV).
pub const NO_KEYRING: &str = "a riff session has no keyring: its secrets come in its \
     environment from riff workers run";

/// True when this process is in a session with its secrets in the
/// environment: [`GRANT_VAR`] is set. Then riff never opens the keyring
/// (01M4CVXJ7ZDAVRKJ8Y59R3KPDV).
pub fn in_session() -> bool {
    std::env::var_os(GRANT_VAR).is_some_and(|v| !v.is_empty())
}

/// The secrets of one session: what the wrapper gives `claude`.
pub struct Secrets {
    key: Key,
    grant: String,
    user: String,
    claude_token: Option<String>,
}

impl fmt::Debug for Secrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secrets")
            .field("key", &self.key.thumbprint())
            .field("grant", &"<hidden>")
            .field("user", &self.user)
            .field("claude_token", &self.claude_token.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

impl Secrets {
    /// The secrets from their parts.
    pub fn new(key: Key, grant: String, user: String, claude_token: Option<String>) -> Self {
        Secrets {
            key,
            grant,
            user,
            claude_token,
        }
    }

    /// Makes the secrets of `session` at the server of `api`, from the
    /// sign-in of the person in the keyring, with no Claude plan token.
    /// Only the wrapper, outside the sandbox, calls it.
    pub async fn make(api: &Api, session: &str) -> Result<Secrets> {
        api.check_riff().await?;
        let key = Key::generate();
        let reply = grant(api, session, &key).await?;
        Ok(Secrets::new(key, reply.access_token, reply.user, None))
    }

    /// The pairs of the environment of `claude`.
    pub fn env(&self) -> Vec<(&'static str, String)> {
        let mut env = vec![
            (KEY_VAR, self.key.to_secret()),
            (GRANT_VAR, self.grant.clone()),
            (USER_VAR, self.user.clone()),
        ];
        env.extend(
            self.claude_token
                .clone()
                .map(|token| (CLAUDE_TOKEN_VAR, token)),
        );
        env
    }

    /// True when the person keeps a Claude plan token.
    pub fn has_claude_token(&self) -> bool {
        self.claude_token.is_some()
    }
}

/// The environment of the secrets of `session` for `claude`
/// ([`Secrets::env`]): the session key, the grant and the user when this
/// machine has a sign-in at the server of `api`, and the Claude plan
/// token when the person keeps one. Only the wrapper, outside the
/// sandbox, calls it. A failure is one line on stderr: `claude` starts
/// with fewer secrets, and its riff calls fail with a clear error.
pub async fn session_env(api: &Api, session: &str) -> Vec<(&'static str, String)> {
    let signed_in = secrets::has_keyring()
        && login::stored(api.base()).is_ok_and(|s| s.is_some_and(|s| !s.ended()));
    let mut secrets = None;
    if signed_in {
        match Secrets::make(api, session).await {
            Ok(made) => secrets = Some(made),
            Err(e) => eprintln!("riff: the session has no riff token: {e:#}"),
        }
    }
    let claude = claude_token().unwrap_or_else(|e| {
        eprintln!("riff: {e:#}");
        None
    });
    let mut env = secrets.map_or_else(Vec::new, |s| s.env());
    env.extend(claude.map(|token| (CLAUDE_TOKEN_VAR, token)));
    env
}

/// The session key and the grant of this process, from the
/// environment: `None` outside a session with a grant. A key that does
/// not read is an error that names the variable, not its value.
pub fn from_env() -> Result<Option<(Key, String)>> {
    if !in_session() {
        return Ok(None);
    }
    let grant = std::env::var(GRANT_VAR).unwrap_or_default();
    let key = std::env::var(KEY_VAR).with_context(|| format!("{GRANT_VAR} has no {KEY_VAR}"))?;
    let key = Key::from_secret(&key).map_err(|_| anyhow::anyhow!("{KEY_VAR} is not a key"))?;
    Ok(Some((key, grant)))
}

/// Asks the server of `api` for a session grant of `session` on the
/// session key `key`, with the person token and the device key of the
/// keyring. When the server does not take the person access token, it
/// refreshes the person pair once and asks again.
async fn grant(api: &Api, session: &str, key: &Key) -> Result<TokenReply> {
    let person = login::access_token(api).await?;
    match ask(api, session, key, &person).await {
        Err(e)
            if e.downcast_ref::<TokenRefused>()
                .is_some_and(TokenRefused::ended) =>
        {
            login::forget(api.base(), &person).await?;
            let person = login::access_token(api).await?;
            ask(api, session, key, &person).await
        }
        reply => reply,
    }
}

async fn ask(api: &Api, session: &str, key: &Key, person: &str) -> Result<TokenReply> {
    let url = format!("{}/v1/token", api.base());
    let request = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(person.to_owned()),
        subject_token_type: Some(ACCESS_TOKEN_TYPE.into()),
        session: Some(session.to_owned()),
        requested_token_type: Some(GRANT_TOKEN_TYPE.into()),
        session_proof: Some(key.proof("POST", &url, None, now())),
        ..TokenRequest::default()
    };
    let reply = api
        .token(&request, &device::key(api.base())?)
        .await
        .context("riff-server gave no session grant")?;
    if reply.token_type != GRANT_TOKEN_TYPE {
        bail!("riff-server gave no session grant: it is of an older version");
    }
    Ok(reply)
}

/// Swaps the session grant `grant` for a session access token, with a
/// proof of the session key `key` (01M4CVXJ3GCEB7B4632J7DD84A).
pub async fn access_token(api: &Api, key: &Key, grant: &str) -> Result<TokenReply> {
    let request = TokenRequest {
        grant_type: TOKEN_EXCHANGE.into(),
        subject_token: Some(grant.to_owned()),
        subject_token_type: Some(GRANT_TOKEN_TYPE.into()),
        ..TokenRequest::default()
    };
    api.token(&request, key)
        .await
        .context("riff-server did not take the session grant: start the session again")
}

/// The Claude plan token that the person keeps on this machine
/// (01M4CVXJCGRC59VVZEHBFA9MPG), or `None`. `riff claude-token` keeps it.
pub fn claude_token() -> Result<Option<String>> {
    secrets::get(CLAUDE_TOKEN_SECRET).context("riff cannot read the Claude plan token")
}

/// The Claude plan token in the text of `claude setup-token`: the
/// first word that starts with `sk-ant-`. The other lines and the
/// spaces do not count.
///
/// ```
/// assert_eq!(
///     riff::grant::token_of("Your token:\n\n  sk-ant-oat01-abc  \n\nKeep it.\n"),
///     Some("sk-ant-oat01-abc".to_owned()),
/// );
/// assert_eq!(riff::grant::token_of("sk-ant-oat01-abc"), Some("sk-ant-oat01-abc".to_owned()));
/// assert_eq!(riff::grant::token_of("no token here\n"), None);
/// ```
pub fn token_of(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|w| w.starts_with("sk-ant-"))
        .map(str::to_owned)
}

/// Keeps the Claude plan token of the person on this machine.
pub fn keep_claude_token(token: &str) -> Result<()> {
    secrets::set(CLAUDE_TOKEN_SECRET, token)
}

/// Removes the Claude plan token of the person from this machine.
/// Returns false when there was none.
pub fn remove_claude_token() -> Result<bool> {
    let had = secrets::get(CLAUDE_TOKEN_SECRET)?.is_some();
    secrets::delete(CLAUDE_TOKEN_SECRET)?;
    Ok(had)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
