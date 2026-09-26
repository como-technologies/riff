//! The riff tokens that `riff-server` issues (R16, R17, R33).
//!
//! # Model
//!
//! ```text
//!  sign-in (user, last use)
//!    ├─ access 1    expires after ACCESS_TTL
//!    ├─ refresh 1   used: it gave pair 2
//!    ├─ access 2
//!    └─ refresh 2   not used yet
//! ```
//!
//! A sign-in holds the user. Each pair of tokens belongs to one sign-in.
//! A token is 32 random bytes in URL-safe base64. The server keeps only
//! the SHA-256 hash of each token, never the token.
//!
//! # Rules
//!
//! - An access token expires [`ACCESS_TTL`] after it is issued.
//! - A refresh token works once. It gives a new pair, and the server
//!   marks it as used.
//! - A used refresh token that comes back revokes the sign-in: each of
//!   its access and refresh tokens stops working.
//! - A sign-in expires when no refresh token of it is used for
//!   [`REFRESH_IDLE`].
//! - A token that the server does not know is refused. After a restart
//!   the server knows no token, so each person signs in again.
//!
//! The store does no I/O and reads no clock. The caller passes `now`.
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_server::token::{Refused, Tokens};
//!
//! let now = Instant::now();
//! let mut tokens = Tokens::default();
//! let first = tokens.sign_in("mike", now).unwrap();
//! assert_eq!(tokens.check(&first.access_token, now).unwrap(), "mike");
//!
//! // A refresh token gives a new pair once.
//! let second = tokens.refresh(&first.refresh_token, now).unwrap();
//! assert_eq!(tokens.check(&second.access_token, now).unwrap(), "mike");
//!
//! // A second use revokes the sign-in.
//! assert_eq!(tokens.refresh(&first.refresh_token, now), Err(Refused::Reused));
//! assert_eq!(tokens.check(&second.access_token, now), Err(Refused::Unknown));
//! ```

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use riff_core::name::{NameError, check};
use riff_core::wire::TokenReply;
use sha2::{Digest, Sha256};

/// An access token works this long (R17).
pub const ACCESS_TTL: Duration = Duration::from_secs(10 * 60);

/// A sign-in ends when no refresh token of it is used this long (R80).
pub const REFRESH_IDLE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Why the server refuses a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The server does not know the token, or its sign-in ended.
    Unknown,
    /// The access token or the sign-in expired.
    Expired,
    /// The refresh token was used before. The sign-in is now revoked.
    Reused,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::Unknown => "the token is not known",
            Refused::Expired => "the token expired",
            Refused::Reused => "the refresh token was used before; sign in again",
        })
    }
}

impl std::error::Error for Refused {}

type Hash = [u8; 32];

/// All tokens of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct Tokens {
    sign_ins: HashMap<u64, SignIn>,
    access: HashMap<Hash, Access>,
    refresh: HashMap<Hash, Refresh>,
    next_sign_in: u64,
}

struct SignIn {
    user: String,
    last_used: Instant,
}

struct Access {
    sign_in: u64,
    expires: Instant,
}

struct Refresh {
    sign_in: u64,
    used: bool,
}

impl Tokens {
    /// Starts a sign-in for `user` and issues its first pair. The caller
    /// has checked the identity of the user.
    pub fn sign_in(&mut self, user: &str, now: Instant) -> Result<TokenReply, NameError> {
        check("user", user)?;
        self.sweep(now);
        let id = self.next_sign_in;
        self.next_sign_in += 1;
        self.sign_ins.insert(
            id,
            SignIn {
                user: user.to_owned(),
                last_used: now,
            },
        );
        Ok(self.issue(id, now))
    }

    /// Swaps a refresh token for a new pair. A refresh token works once.
    pub fn refresh(&mut self, token: &str, now: Instant) -> Result<TokenReply, Refused> {
        self.sweep(now);
        let refresh = self.refresh.get_mut(&hash(token)).ok_or(Refused::Unknown)?;
        let id = refresh.sign_in;
        if refresh.used {
            self.revoke(id);
            return Err(Refused::Reused);
        }
        refresh.used = true;
        let sign_in = self.sign_ins.get_mut(&id).ok_or(Refused::Unknown)?;
        sign_in.last_used = now;
        Ok(self.issue(id, now))
    }

    /// Returns the user of a live access token.
    pub fn check(&self, token: &str, now: Instant) -> Result<&str, Refused> {
        let access = self.access.get(&hash(token)).ok_or(Refused::Unknown)?;
        let sign_in = self.sign_ins.get(&access.sign_in).ok_or(Refused::Unknown)?;
        if now >= access.expires {
            return Err(Refused::Expired);
        }
        Ok(&sign_in.user)
    }

    fn issue(&mut self, sign_in: u64, now: Instant) -> TokenReply {
        let access_token = random_token();
        let refresh_token = random_token();
        self.access.insert(
            hash(&access_token),
            Access {
                sign_in,
                expires: now + ACCESS_TTL,
            },
        );
        self.refresh.insert(
            hash(&refresh_token),
            Refresh {
                sign_in,
                used: false,
            },
        );
        TokenReply {
            access_token,
            token_type: "Bearer".into(),
            expires_in: ACCESS_TTL.as_secs(),
            refresh_token,
        }
    }

    /// Ends one sign-in and each token of it.
    fn revoke(&mut self, sign_in: u64) {
        self.sign_ins.remove(&sign_in);
        self.access.retain(|_, a| a.sign_in != sign_in);
        self.refresh.retain(|_, r| r.sign_in != sign_in);
    }

    /// Forgets expired access tokens and idle sign-ins.
    fn sweep(&mut self, now: Instant) {
        self.sign_ins
            .retain(|_, s| now.duration_since(s.last_used) < REFRESH_IDLE);
        let live = &self.sign_ins;
        self.access
            .retain(|_, a| now < a.expires && live.contains_key(&a.sign_in));
        self.refresh.retain(|_, r| live.contains_key(&r.sign_in));
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    // The OS random source fails only when the OS is broken.
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash(token: &str) -> Hash {
    Sha256::digest(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_in() -> (Tokens, TokenReply, Instant) {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        let pair = tokens.sign_in("mike", now).unwrap();
        (tokens, pair, now)
    }

    #[test]
    fn access_token_names_the_user() {
        let (tokens, pair, now) = signed_in();
        assert_eq!(tokens.check(&pair.access_token, now), Ok("mike"));
        assert_eq!(pair.token_type, "Bearer");
        assert_eq!(pair.expires_in, 600);
    }

    #[test]
    fn access_token_expires_after_ten_minutes() {
        let (tokens, pair, now) = signed_in();
        let almost = now + ACCESS_TTL - Duration::from_secs(1);
        assert_eq!(tokens.check(&pair.access_token, almost), Ok("mike"));
        assert_eq!(
            tokens.check(&pair.access_token, now + ACCESS_TTL),
            Err(Refused::Expired)
        );
    }

    #[test]
    fn unknown_tokens_are_refused() {
        let (mut tokens, pair, now) = signed_in();
        assert_eq!(tokens.check("nope", now), Err(Refused::Unknown));
        assert_eq!(tokens.refresh("nope", now), Err(Refused::Unknown));
        // Each kind of token works only in its own place.
        assert_eq!(
            tokens.check(&pair.refresh_token, now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&pair.access_token, now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn a_new_server_knows_no_token() {
        let (_, pair, now) = signed_in();
        let mut restarted = Tokens::default();
        assert_eq!(
            restarted.check(&pair.access_token, now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            restarted.refresh(&pair.refresh_token, now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn refresh_rotates_the_pair() {
        let (mut tokens, first, now) = signed_in();
        let later = now + ACCESS_TTL;
        let second = tokens.refresh(&first.refresh_token, later).unwrap();
        assert_ne!(second.access_token, first.access_token);
        assert_ne!(second.refresh_token, first.refresh_token);
        assert_eq!(tokens.check(&second.access_token, later), Ok("mike"));
        let third = tokens.refresh(&second.refresh_token, later).unwrap();
        assert_eq!(tokens.check(&third.access_token, later), Ok("mike"));
    }

    #[test]
    fn reuse_revokes_the_sign_in() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, now),
            Err(Refused::Reused)
        );
        assert_eq!(
            tokens.check(&second.access_token, now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&second.refresh_token, now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn reuse_leaves_other_sign_ins_alone() {
        let (mut tokens, first, now) = signed_in();
        let other = tokens.sign_in("mike", now).unwrap();
        tokens.refresh(&first.refresh_token, now).unwrap();
        tokens.refresh(&first.refresh_token, now).unwrap_err();
        assert_eq!(tokens.check(&other.access_token, now), Ok("mike"));
        assert!(tokens.refresh(&other.refresh_token, now).is_ok());
    }

    #[test]
    fn an_idle_sign_in_expires() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens
            .refresh(&first.refresh_token, now + REFRESH_IDLE / 2)
            .unwrap();
        let idle = now + REFRESH_IDLE / 2 + REFRESH_IDLE;
        assert_eq!(
            tokens.refresh(&second.refresh_token, idle),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn sweep_forgets_expired_access_tokens() {
        let (mut tokens, _, now) = signed_in();
        tokens.sign_in("brett", now + ACCESS_TTL).unwrap();
        assert_eq!(tokens.access.len(), 1);
        assert_eq!(tokens.sign_ins.len(), 2);
    }

    #[test]
    fn bad_user_names_are_refused() {
        let mut tokens = Tokens::default();
        assert!(tokens.sign_in("", Instant::now()).is_err());
        assert!(tokens.sign_in("a b", Instant::now()).is_err());
    }

    #[test]
    fn tokens_are_random_and_url_safe() {
        let a = random_token();
        assert_ne!(a, random_token());
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}
