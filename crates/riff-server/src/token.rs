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
//! A sign-in holds the user and the thumbprint of the device key (R18).
//! Each pair of tokens belongs to one sign-in.
//!
//! A pair is a *person* pair or a *session* pair (R19). `riff login`
//! gets a person pair. [`Tokens::for_session`] swaps a live person
//! access token for a session pair. A session pair has its own refresh
//! chain in the same sign-in. So [`Tokens::caller`] gives a [`Who`]
//! with the user and, for a session pair, the session ID.
//! A token is 32 random bytes in URL-safe base64. The server keeps only
//! the SHA-256 hash of each token, never the token.
//!
//! # Rules
//!
//! - Each token works only with the device key of its sign-in. The
//!   caller checks the DPoP proof and passes the thumbprint of its key
//!   (see [`riff_core::dpop`]).
//! - An access token expires [`ACCESS_TTL`] after it is issued.
//! - A refresh token works once. It gives a new pair, and the server
//!   marks it as used.
//! - A used refresh token that comes back revokes the sign-in: each of
//!   its access and refresh tokens stops working. The server keeps a
//!   used refresh token for [`REUSE_WINDOW`]. After that, it is not
//!   known (R116).
//! - A sign-in expires when no refresh token of it is used for
//!   [`REFRESH_IDLE`].
//! - A refresh gives a pair of the same kind: a session pair stays for
//!   its session.
//! - Only a person access token gives a session pair (R105).
//! - [`Tokens::revoke_user`] ends each sign-in of one person at once
//!   (R20). This ends the session pairs too.
//! - A token that the server does not know is refused.
//!
//! The store does no I/O and reads no clock. The caller passes `now`.
//!
//! # Saved form
//!
//! [`Tokens::to_bytes`] gives the store as JSON, and [`Tokens::from_bytes`]
//! loads it again (R124). The JSON holds only the hash of each token
//! (R81). Each time in it is a wall-clock time, so the time that the
//! server was down counts. The store keeps each time as a deadline in the
//! future, so a new process can always hold it.
//!
//! ```
//! use std::time::{Instant, SystemTime};
//! use riff_server::token::Tokens;
//!
//! let (now, wall) = (Instant::now(), SystemTime::now());
//! let mut tokens = Tokens::default();
//! let pair = tokens.sign_in("mike", "k", now).unwrap();
//! let bytes = tokens.to_bytes(now, wall);
//! assert!(!String::from_utf8_lossy(&bytes).contains(&pair.access_token));
//!
//! let loaded = Tokens::from_bytes(&bytes, Instant::now(), SystemTime::now()).unwrap();
//! assert_eq!(loaded.check(&pair.access_token, "k", Instant::now()).unwrap(), "mike");
//! ```
//!
//! # Example
//!
//! ```
//! use std::time::Instant;
//! use riff_server::token::{Refused, Tokens};
//!
//! let now = Instant::now();
//! let mut tokens = Tokens::default();
//! let first = tokens.sign_in("mike", "jkt-laptop", now).unwrap();
//! assert_eq!(tokens.check(&first.access_token, "jkt-laptop", now).unwrap(), "mike");
//!
//! // Another device key cannot use the token.
//! assert_eq!(tokens.check(&first.access_token, "jkt-thief", now), Err(Refused::WrongKey));
//!
//! // A refresh token gives a new pair once.
//! let second = tokens.refresh(&first.refresh_token, "jkt-laptop", now).unwrap();
//! assert_eq!(tokens.check(&second.access_token, "jkt-laptop", now).unwrap(), "mike");
//!
//! // A session pair acts only as its session.
//! let session = tokens.for_session(&second.access_token, "jkt-laptop", "a6cf", now).unwrap();
//! let who = tokens.caller(&session.access_token, "jkt-laptop", now).unwrap();
//! assert_eq!(who.to_string(), "mike/a6cf");
//!
//! // A second use revokes the sign-in.
//! assert_eq!(tokens.refresh(&first.refresh_token, "jkt-laptop", now), Err(Refused::Reused));
//! assert_eq!(tokens.check(&second.access_token, "jkt-laptop", now), Err(Refused::Unknown));
//! assert_eq!(tokens.check(&session.access_token, "jkt-laptop", now), Err(Refused::Unknown));
//! ```

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use riff_core::name::{NameError, Who};
use riff_core::wire::TokenReply;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// An access token works this long (R17).
pub const ACCESS_TTL: Duration = Duration::from_secs(10 * 60);

/// A sign-in ends when no refresh token of it is used this long (R80).
pub const REFRESH_IDLE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The server keeps a used refresh token this long, to find reuse
/// (R116).
pub const REUSE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// Why the server refuses a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The server does not know the token, or its sign-in ended.
    Unknown,
    /// The access token or the sign-in expired.
    Expired,
    /// The refresh token was used before. The sign-in is now revoked.
    Reused,
    /// The token belongs to another device key.
    WrongKey,
    /// A session token cannot give another token (R105).
    NotPerson,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::Unknown => "the token is not known",
            Refused::Expired => "the token expired",
            Refused::Reused => "the refresh token was used before; sign in again",
            Refused::WrongKey => "the token belongs to another device key",
            Refused::NotPerson => "only a person token gives a session token",
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
    /// The thumbprint of the device key.
    jkt: String,
    /// The sign-in ends at this time, unless a refresh comes first.
    idle_until: Instant,
}

struct Access {
    sign_in: u64,
    /// The session of a session token. `None` for a person token.
    session: Option<String>,
    expires: Instant,
}

struct Refresh {
    sign_in: u64,
    session: Option<String>,
    /// For a used token: the server forgets it at this time.
    used: Option<Instant>,
}

impl Tokens {
    /// Starts a sign-in for `user` on the device key `jkt`, and issues
    /// its first pair. The caller has checked the identity of the user
    /// and the proof of the key.
    pub fn sign_in(
        &mut self,
        user: &str,
        jkt: &str,
        now: Instant,
    ) -> Result<TokenReply, NameError> {
        Who::new(user, None)?;
        self.sweep(now);
        let id = self.next_sign_in;
        self.next_sign_in += 1;
        self.sign_ins.insert(
            id,
            SignIn {
                user: user.to_owned(),
                jkt: jkt.to_owned(),
                idle_until: now + REFRESH_IDLE,
            },
        );
        Ok(self.issue(id, None, now))
    }

    /// True when `token` is a refresh token of a live sign-in on the
    /// device key `jkt`, used or not. It changes nothing.
    pub fn knows_refresh(&self, token: &str, jkt: &str) -> bool {
        self.refresh
            .get(&hash(token))
            .and_then(|r| self.sign_ins.get(&r.sign_in))
            .is_some_and(|s| s.jkt == jkt)
    }

    /// Swaps a refresh token for a new pair. A refresh token works once,
    /// and only with the device key `jkt` of its sign-in.
    pub fn refresh(&mut self, token: &str, jkt: &str, now: Instant) -> Result<TokenReply, Refused> {
        self.sweep(now);
        let refresh = self.refresh.get_mut(&hash(token)).ok_or(Refused::Unknown)?;
        let id = refresh.sign_in;
        let sign_in = self.sign_ins.get_mut(&id).ok_or(Refused::Unknown)?;
        // Only the device key can end the sign-in by reuse (R110).
        if sign_in.jkt != jkt {
            return Err(Refused::WrongKey);
        }
        if refresh.used.is_some() {
            self.revoke(id);
            return Err(Refused::Reused);
        }
        sign_in.idle_until = now + REFRESH_IDLE;
        refresh.used = Some(now + REUSE_WINDOW);
        let session = refresh.session.clone();
        Ok(self.issue(id, session, now))
    }

    /// Swaps a live person access token for a session pair in the same
    /// sign-in (R19). The session pair works only for `session`, and
    /// only with the device key `jkt`.
    pub fn for_session(
        &mut self,
        token: &str,
        jkt: &str,
        session: &str,
        now: Instant,
    ) -> Result<TokenReply, Refused> {
        let who = self.caller(token, jkt, now)?;
        if who.session().is_some() {
            return Err(Refused::NotPerson);
        }
        Who::new(who.user(), Some(session)).map_err(|_| Refused::Unknown)?;
        self.sweep(now);
        let id = self.access[&hash(token)].sign_in;
        Ok(self.issue(id, Some(session.to_owned()), now))
    }

    /// Returns the user of a live access token, used with the device key
    /// `jkt`.
    pub fn check(&self, token: &str, jkt: &str, now: Instant) -> Result<String, Refused> {
        self.caller(token, jkt, now)
            .map(|who| who.user().to_owned())
    }

    /// Returns who a live access token acts as, used with the device key
    /// `jkt`: the user, and the session of a session token.
    pub fn caller(&self, token: &str, jkt: &str, now: Instant) -> Result<Who, Refused> {
        let access = self.access.get(&hash(token)).ok_or(Refused::Unknown)?;
        let sign_in = self.sign_ins.get(&access.sign_in).ok_or(Refused::Unknown)?;
        if sign_in.jkt != jkt {
            return Err(Refused::WrongKey);
        }
        if now >= access.expires {
            return Err(Refused::Expired);
        }
        // The store checked both parts when it issued the token.
        Who::new(&sign_in.user, access.session.as_deref()).map_err(|_| Refused::Unknown)
    }

    /// Ends each sign-in of `user` and each token of them (R20).
    /// Returns the number of sign-ins that ended.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let laptop = tokens.sign_in("mike", "k", now).unwrap();
    /// let desktop = tokens.sign_in("mike", "k", now).unwrap();
    /// assert_eq!(tokens.revoke_user("mike"), 2);
    /// assert_eq!(tokens.check(&laptop.access_token, "k", now), Err(Refused::Unknown));
    /// assert_eq!(tokens.check(&desktop.access_token, "k", now), Err(Refused::Unknown));
    /// ```
    pub fn revoke_user(&mut self, user: &str) -> usize {
        let ids: Vec<u64> = self
            .sign_ins
            .iter()
            .filter(|(_, s)| s.user == user)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.revoke(*id);
        }
        ids.len()
    }

    /// The store as JSON, with only the hash of each token (R81). `now`
    /// and `wall` are the same time on the two clocks.
    pub fn to_bytes(&self, now: Instant, wall: SystemTime) -> Vec<u8> {
        let clock = Clock { now, wall };
        let live = |t: Instant| (now < t).then(|| clock.save(t));
        let saved = Saved {
            next_sign_in: self.next_sign_in,
            sign_ins: self
                .sign_ins
                .iter()
                .filter_map(|(id, s)| {
                    Some(SavedSignIn {
                        id: *id,
                        user: s.user.clone(),
                        jkt: s.jkt.clone(),
                        idle_until: live(s.idle_until)?,
                    })
                })
                .collect(),
            access: self
                .access
                .iter()
                .filter_map(|(hash, a)| {
                    Some(SavedAccess {
                        hash: URL_SAFE_NO_PAD.encode(hash),
                        sign_in: a.sign_in,
                        session: a.session.clone(),
                        expires: live(a.expires)?,
                    })
                })
                .collect(),
            refresh: self
                .refresh
                .iter()
                .filter_map(|(hash, r)| {
                    Some(SavedRefresh {
                        hash: URL_SAFE_NO_PAD.encode(hash),
                        sign_in: r.sign_in,
                        session: r.session.clone(),
                        used: match r.used {
                            Some(forget) => Some(live(forget)?),
                            None => None,
                        },
                    })
                })
                .collect(),
        };
        serde_json::to_vec(&saved).expect("the saved form is JSON")
    }

    /// Loads a store from [`Tokens::to_bytes`]. It drops each token and
    /// sign-in that ended while the server was down.
    pub fn from_bytes(bytes: &[u8], now: Instant, wall: SystemTime) -> Result<Tokens, LoadError> {
        let saved: Saved = serde_json::from_slice(bytes).map_err(|e| LoadError(e.to_string()))?;
        let clock = Clock { now, wall };
        let mut tokens = Tokens {
            next_sign_in: saved.next_sign_in,
            ..Tokens::default()
        };
        for s in saved.sign_ins {
            if s.id >= saved.next_sign_in {
                return Err(LoadError(format!(
                    "sign-in {} is not below next_sign_in",
                    s.id
                )));
            }
            if let Some(idle_until) = clock.load(s.idle_until) {
                let sign_in = SignIn {
                    user: s.user,
                    jkt: s.jkt,
                    idle_until,
                };
                tokens.sign_ins.insert(s.id, sign_in);
            }
        }
        for a in saved.access {
            if let Some(expires) = clock.load(a.expires) {
                let access = Access {
                    sign_in: a.sign_in,
                    session: a.session,
                    expires,
                };
                tokens.access.insert(unhash(&a.hash)?, access);
            }
        }
        for r in saved.refresh {
            let used = match r.used {
                None => None,
                Some(forget) => match clock.load(forget) {
                    Some(forget) => Some(forget),
                    None => continue,
                },
            };
            let refresh = Refresh {
                sign_in: r.sign_in,
                session: r.session,
                used,
            };
            tokens.refresh.insert(unhash(&r.hash)?, refresh);
        }
        tokens.sweep(now);
        Ok(tokens)
    }

    fn issue(&mut self, sign_in: u64, session: Option<String>, now: Instant) -> TokenReply {
        let user = self
            .sign_ins
            .get(&sign_in)
            .map_or_else(String::new, |s| s.user.clone());
        let access_token = random_token();
        let refresh_token = random_token();
        self.access.insert(
            hash(&access_token),
            Access {
                sign_in,
                session: session.clone(),
                expires: now + ACCESS_TTL,
            },
        );
        self.refresh.insert(
            hash(&refresh_token),
            Refresh {
                sign_in,
                session,
                used: None,
            },
        );
        TokenReply {
            access_token,
            token_type: "DPoP".into(),
            expires_in: ACCESS_TTL.as_secs(),
            refresh_token,
            user,
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
        self.sign_ins.retain(|_, s| now < s.idle_until);
        let live = &self.sign_ins;
        self.access
            .retain(|_, a| now < a.expires && live.contains_key(&a.sign_in));
        self.refresh.retain(|_, r| {
            live.contains_key(&r.sign_in) && r.used.is_none_or(|forget| now < forget)
        });
    }
}

/// Why [`Tokens::from_bytes`] failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError(String);

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the saved token store is not valid: {}", self.0)
    }
}

impl std::error::Error for LoadError {}

/// The saved form. Each time is in milliseconds since the Unix epoch.
#[derive(Serialize, Deserialize)]
struct Saved {
    next_sign_in: u64,
    sign_ins: Vec<SavedSignIn>,
    access: Vec<SavedAccess>,
    refresh: Vec<SavedRefresh>,
}

#[derive(Serialize, Deserialize)]
struct SavedSignIn {
    id: u64,
    user: String,
    jkt: String,
    idle_until: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedAccess {
    hash: String,
    sign_in: u64,
    session: Option<String>,
    expires: u64,
}

#[derive(Serialize, Deserialize)]
struct SavedRefresh {
    hash: String,
    sign_in: u64,
    session: Option<String>,
    used: Option<u64>,
}

/// One time on the two clocks. It turns a deadline into a wall-clock
/// time and back.
struct Clock {
    now: Instant,
    wall: SystemTime,
}

impl Clock {
    fn save(&self, deadline: Instant) -> u64 {
        let wall = self.wall + deadline.saturating_duration_since(self.now);
        let since = wall.duration_since(UNIX_EPOCH).unwrap_or_default();
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    }

    /// The deadline, or `None` when it is not in the future.
    fn load(&self, millis: u64) -> Option<Instant> {
        let wall = UNIX_EPOCH.checked_add(Duration::from_millis(millis))?;
        let left = wall.duration_since(self.wall).ok()?;
        (!left.is_zero())
            .then(|| self.now.checked_add(left))
            .flatten()
    }
}

fn unhash(text: &str) -> Result<Hash, LoadError> {
    URL_SAFE_NO_PAD
        .decode(text)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| LoadError(format!("{text} is not a token hash")))
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
        let pair = tokens.sign_in("mike", "k", now).unwrap();
        (tokens, pair, now)
    }

    #[test]
    fn access_token_names_the_user() {
        let (tokens, pair, now) = signed_in();
        assert_eq!(
            tokens.check(&pair.access_token, "k", now),
            Ok("mike".to_owned())
        );
        assert_eq!(pair.token_type, "DPoP");
        assert_eq!(pair.expires_in, 600);
        assert_eq!(pair.user, "mike");
    }

    #[test]
    fn access_token_expires_after_ten_minutes() {
        let (tokens, pair, now) = signed_in();
        let almost = now + ACCESS_TTL - Duration::from_secs(1);
        assert_eq!(
            tokens.check(&pair.access_token, "k", almost),
            Ok("mike".to_owned())
        );
        assert_eq!(
            tokens.check(&pair.access_token, "k", now + ACCESS_TTL),
            Err(Refused::Expired)
        );
    }

    #[test]
    fn unknown_tokens_are_refused() {
        let (mut tokens, pair, now) = signed_in();
        assert_eq!(tokens.check("nope", "k", now), Err(Refused::Unknown));
        assert_eq!(tokens.refresh("nope", "k", now), Err(Refused::Unknown));
        // Each kind of token works only in its own place.
        assert_eq!(
            tokens.check(&pair.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&pair.access_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn a_new_server_knows_no_token() {
        let (_, pair, now) = signed_in();
        let mut restarted = Tokens::default();
        assert_eq!(
            restarted.check(&pair.access_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            restarted.refresh(&pair.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn refresh_rotates_the_pair() {
        let (mut tokens, first, now) = signed_in();
        let later = now + ACCESS_TTL;
        let second = tokens.refresh(&first.refresh_token, "k", later).unwrap();
        assert_ne!(second.access_token, first.access_token);
        assert_ne!(second.refresh_token, first.refresh_token);
        assert_eq!(
            tokens.check(&second.access_token, "k", later),
            Ok("mike".to_owned())
        );
        let third = tokens.refresh(&second.refresh_token, "k", later).unwrap();
        assert_eq!(
            tokens.check(&third.access_token, "k", later),
            Ok("mike".to_owned())
        );
    }

    #[test]
    fn reuse_revokes_the_sign_in() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        assert_eq!(
            tokens.check(&second.access_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&second.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn reuse_with_another_key_leaves_the_sign_in() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "thief", now),
            Err(Refused::WrongKey)
        );
        assert_eq!(
            tokens.check(&second.access_token, "k", now),
            Ok("mike".to_owned())
        );
    }

    #[test]
    fn reuse_leaves_other_sign_ins_alone() {
        let (mut tokens, first, now) = signed_in();
        let other = tokens.sign_in("mike", "k", now).unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap_err();
        assert_eq!(
            tokens.check(&other.access_token, "k", now),
            Ok("mike".to_owned())
        );
        assert!(tokens.refresh(&other.refresh_token, "k", now).is_ok());
    }

    #[test]
    fn revoke_user_ends_only_that_person() {
        let (mut tokens, mike, now) = signed_in();
        let brett = tokens.sign_in("brett", "k", now).unwrap();
        assert_eq!(tokens.revoke_user("mike"), 1);
        assert_eq!(
            tokens.check(&mike.access_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&mike.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.check(&brett.access_token, "k", now),
            Ok("brett".to_owned())
        );
        assert_eq!(tokens.revoke_user("mike"), 0);
    }

    #[test]
    fn a_person_signs_in_again_after_revoke() {
        let (mut tokens, _, now) = signed_in();
        tokens.revoke_user("mike");
        let again = tokens.sign_in("mike", "k", now).unwrap();
        assert_eq!(
            tokens.check(&again.access_token, "k", now),
            Ok("mike".to_owned())
        );
    }

    #[test]
    fn an_idle_sign_in_expires() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens
            .refresh(&first.refresh_token, "k", now + REFRESH_IDLE / 2)
            .unwrap();
        let idle = now + REFRESH_IDLE / 2 + REFRESH_IDLE;
        assert_eq!(
            tokens.refresh(&second.refresh_token, "k", idle),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn sweep_forgets_expired_access_tokens() {
        let (mut tokens, _, now) = signed_in();
        tokens.sign_in("brett", "k", now + ACCESS_TTL).unwrap();
        assert_eq!(tokens.access.len(), 1);
        assert_eq!(tokens.sign_ins.len(), 2);
    }

    #[test]
    fn sweep_forgets_old_used_refresh_tokens() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(tokens.refresh.len(), 2);
        let later = now + REUSE_WINDOW;
        let third = tokens.refresh(&second.refresh_token, "k", later).unwrap();
        // The first token is gone. The second is used but new enough.
        assert_eq!(tokens.refresh.len(), 2);
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", later),
            Err(Refused::Unknown)
        );
        assert!(tokens.refresh(&third.refresh_token, "k", later).is_ok());
    }

    #[test]
    fn a_token_works_only_with_its_device_key() {
        let (mut tokens, pair, now) = signed_in();
        assert_eq!(
            tokens.check(&pair.access_token, "thief", now),
            Err(Refused::WrongKey)
        );
        assert_eq!(
            tokens.refresh(&pair.refresh_token, "thief", now),
            Err(Refused::WrongKey)
        );
        // The refused refresh did not use the token.
        assert!(tokens.refresh(&pair.refresh_token, "k", now).is_ok());
    }

    #[test]
    fn bad_user_names_are_refused() {
        let mut tokens = Tokens::default();
        assert!(tokens.sign_in("", "k", Instant::now()).is_err());
        assert!(tokens.sign_in("a b", "k", Instant::now()).is_err());
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

    #[test]
    fn a_session_pair_acts_only_as_its_session() {
        let (mut tokens, person, now) = signed_in();
        let a = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        let b = tokens
            .for_session(&person.access_token, "k", "b", now)
            .unwrap();
        let who = |t: &str| tokens.caller(t, "k", now).unwrap().to_string();
        assert_eq!(who(&person.access_token), "mike");
        assert_eq!(who(&a.access_token), "mike/a");
        assert_eq!(who(&b.access_token), "mike/b");
        assert_eq!(a.user, "mike");
    }

    #[test]
    fn a_session_refresh_keeps_the_session() {
        let (mut tokens, person, now) = signed_in();
        let first = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let who = tokens.caller(&second.access_token, "k", now).unwrap();
        assert_eq!(who.session(), Some("a"));
    }

    #[test]
    fn only_a_person_token_gives_a_session_pair() {
        let (mut tokens, person, now) = signed_in();
        let a = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        assert_eq!(
            tokens.for_session(&a.access_token, "k", "b", now),
            Err(Refused::NotPerson)
        );
        assert_eq!(
            tokens.for_session(&person.access_token, "other", "b", now),
            Err(Refused::WrongKey)
        );
        assert_eq!(
            tokens.for_session(&person.refresh_token, "k", "b", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.for_session(&person.access_token, "k", "bad id", now),
            Err(Refused::Unknown)
        );
    }

    /// Saves at `now` and loads `down` later, on new clocks.
    fn restart(tokens: &Tokens, now: Instant, down: Duration) -> (Tokens, Instant) {
        let wall = SystemTime::now();
        let bytes = tokens.to_bytes(now, wall);
        let later = Instant::now() + Duration::from_secs(3600);
        (
            Tokens::from_bytes(&bytes, later, wall + down).unwrap(),
            later,
        )
    }

    #[test]
    fn tokens_work_after_a_restart() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let session = tokens
            .for_session(&second.access_token, "k", "a", now)
            .unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        let who = |t: &Tokens, token: &str| t.caller(token, "k", now).unwrap().to_string();
        assert_eq!(who(&loaded, &second.access_token), "mike");
        assert_eq!(who(&loaded, &session.access_token), "mike/a");
        let third = loaded.refresh(&session.refresh_token, "k", now).unwrap();
        assert_eq!(who(&loaded, &third.access_token), "mike/a");
        assert_eq!(
            loaded.check(&second.access_token, "thief", now),
            Err(Refused::WrongKey)
        );
    }

    #[test]
    fn a_used_refresh_token_still_revokes_after_a_restart() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        assert_eq!(
            loaded.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        assert_eq!(
            loaded.check(&second.access_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn the_saved_form_holds_no_token() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let text = String::from_utf8(tokens.to_bytes(now, SystemTime::now())).unwrap();
        for token in [
            &first.access_token,
            &first.refresh_token,
            &second.access_token,
            &second.refresh_token,
        ] {
            assert!(!text.contains(token.as_str()), "{text}");
        }
        assert!(text.contains(&URL_SAFE_NO_PAD.encode(hash(&second.access_token))));
    }

    #[test]
    fn the_time_that_the_server_was_down_counts() {
        let (mut tokens, first, now) = signed_in();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (mut loaded, later) = restart(&tokens, now, ACCESS_TTL);
        assert_eq!(
            loaded.check(&first.access_token, "k", later),
            Err(Refused::Unknown)
        );
        assert_eq!(loaded.access.len(), 0);
        // The used refresh token is still known.
        assert_eq!(
            loaded.refresh(&first.refresh_token, "k", later),
            Err(Refused::Reused)
        );
    }

    #[test]
    fn a_restart_forgets_what_ended_while_the_server_was_down() {
        let (mut tokens, first, now) = signed_in();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (loaded, _) = restart(&tokens, now, REUSE_WINDOW);
        assert_eq!(loaded.refresh.len(), 1, "only the unused refresh token");
        let (loaded, _) = restart(&tokens, now, REFRESH_IDLE);
        assert!(loaded.sign_ins.is_empty() && loaded.refresh.is_empty());
    }

    #[test]
    fn a_new_sign_in_after_a_restart_gets_a_new_id() {
        let (mut tokens, first, now) = signed_in();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::ZERO);
        let other = loaded.sign_in("mike", "k", now).unwrap();
        loaded.refresh(&first.refresh_token, "k", now).unwrap_err();
        assert_eq!(
            loaded.check(&other.access_token, "k", now),
            Ok("mike".to_owned())
        );
    }

    #[test]
    fn a_bad_saved_form_does_not_load() {
        let (now, wall) = (Instant::now(), SystemTime::now());
        assert!(Tokens::from_bytes(b"{", now, wall).is_err());
        let bad_hash = br#"{"next_sign_in":1,"sign_ins":[],"access":[],
            "refresh":[{"hash":"abc","sign_in":0,"session":null,"used":null}]}"#;
        assert!(Tokens::from_bytes(bad_hash, now, wall).is_err());
        let bad_id = br#"{"next_sign_in":0,"sign_ins":[
            {"id":0,"user":"mike","jkt":"k","idle_until":18446744073709551615}],
            "access":[],"refresh":[]}"#;
        assert!(Tokens::from_bytes(bad_id, now, wall).is_err());
    }

    #[test]
    fn reuse_of_a_session_refresh_revokes_the_sign_in() {
        let (mut tokens, person, now) = signed_in();
        let first = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap_err();
        assert_eq!(
            tokens.check(&person.access_token, "k", now),
            Err(Refused::Unknown)
        );
    }
}
