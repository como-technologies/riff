//! The riff tokens that `riff-server` issues (R16, R17, R33).
//!
//! # Model
//!
//! ```text
//!  sign-in (user, device key, last use)
//!    ├─ person chain           generation 7
//!    ├─ person access tokens   each expires after ACCESS_TTL
//!    └─ session access tokens  each for one session, with no chain
//! ```
//!
//! A sign-in holds the user, the thumbprint of the device key (R18),
//! and the position of the log at its start
//! (01M3XA87A9GGFA89RQXWSKY0V6). A person signs in with a verified
//! email. The USER comes from the email (R208). The people are not in
//! this store: who may sign in, the email of each USER (R209) and the
//! roles are state of the log (see [`crate::state::people`]). The
//! command `admit` decides, and then [`Tokens::start`] makes the
//! sign-in.
//!
//! An access token is a *person* token or a *session* token (R19).
//! `riff login` gets a person pair: an access token and a refresh
//! token. [`Tokens::for_session`] swaps a live person access token for
//! a session access token. So [`Tokens::caller`] gives a [`Who`] with
//! the user and, for a session token, the session ID.
//!
//! A sign-in has one *chain* of refresh tokens: the person chain. A
//! session token has no refresh token and no chain
//! (01M3WFVAB44T8EP4QZD4KS7DRF). One session has many processes: `riff
//! mcp`, `riff watch`, each hook and each `riff` command. Each process
//! swaps the person token for an access token of its own, and a swap
//! ends no other token. A long process swaps again before its token
//! expires.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as riff mcp (long)
//!     participant H as riff claim (short)
//!     participant S as riff-server
//!     L->>S: swap the person token, session a6cf
//!     S-->>L: access token 1
//!     H->>S: swap the person token, session a6cf
//!     S-->>H: access token 2
//!     Note over S: token 1 stays live
//!     L->>S: a call with token 1
//!     S-->>L: 200
//!     Note over L: before token 1 expires
//!     L->>S: swap the person token, session a6cf
//!     S-->>L: access token 3
//! ```
//!
//! A refresh token names its chain and its generation:
//! `chain.generation.secret`. The chain ID and the secret are random
//! bytes in URL-safe base64. Each refresh gives the next generation.
//!
//! The server keeps only SHA-256 hashes, never a token (R81). For each
//! chain, it keeps the hash of the refresh token of the current
//! generation, and the hash of the one before it, for a lost reply. It
//! keeps nothing for an older generation.
//!
//! # Rules
//!
//! - Each token works only with the device key of its sign-in. The
//!   caller checks the DPoP proof and passes the thumbprint of its key
//!   (see [`riff_core::dpop`]). [`Tokens::refresh`] checks the key
//!   before the generation, so only the device key ends a sign-in by
//!   reuse (R110).
//! - An access token expires [`ACCESS_TTL`] after it is issued. The
//!   access tokens are only in memory: the saved form has none. After a
//!   restart, each client refreshes one time.
//! - The refresh token of the current generation gives the next
//!   generation.
//! - A refresh token of an older generation that comes back revokes the
//!   sign-in: each of its access and refresh tokens stops working
//!   (01M3TFG4SJ5C96NH8W7XRXJG6Z).
//! - A lost reply is not a reuse (01M3MX4TG7PNNETZ986DQS10JJ). The
//!   refresh token of the generation before the current one comes back
//!   when the reply of its use never came: for example a 503, or a
//!   process that stopped. The server then ends the pair of the current
//!   generation, and gives a new pair with the same generation. Only a
//!   reuse after the next refresh revokes the sign-in.
//! - After a load, the saved form can be one generation behind: the
//!   server saves it after the reply of a refresh. So the first refresh
//!   of a loaded chain also takes the next generation as good
//!   (01M3TFG4WE7CZQ4TCJE2NTC52E). The server has no hash of that
//!   token. The device key of the sign-in is the check.
//! - A sign-in expires when no refresh token of it is used for
//!   [`REFRESH_IDLE`].
//! - Only a person access token gives a session token (R105). A new
//!   session token ends no other token.
//! - [`Tokens::end`] ends each sign-in of one person that started
//!   before a position of the log (R20). This ends the session tokens
//!   too. It is the effect of a `member_removed` or a `signins_ended`
//!   record. The store keeps that position for the USER, and
//!   [`Tokens::start`] refuses a sign-in that started before it. The
//!   two are one step, under the lock of the store: no sign-in of the
//!   person starts between them. [`Tokens::drop_ended`] does the same
//!   at a load, so a stop between the write of the record and the
//!   effect lets no removed person in.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as sign-in of bob
//!     participant E as engine
//!     participant W as writer
//!     participant T as token store
//!     P->>E: admit: checked at the position 7
//!     Note over E: an admin sends remove (bob)
//!     E->>W: member_removed at the position 8
//!     W->>T: end (bob, 8): the position of bob is 8
//!     P->>T: start (bob, 7)
//!     T-->>P: refused: 7 is before 8
//! ```
//! - A token that the server does not know is refused. A refresh token
//!   with a wrong secret is not known, and changes nothing.
//!
//! ```mermaid
//! flowchart TD
//!     T[refresh token chain.G.secret] --> K{device key of the sign-in?}
//!     K -- no --> W[refuse: wrong key]
//!     K -- yes --> C{G is the current generation, and the hash matches?}
//!     C -- yes --> N[give generation G+1]
//!     C -- no --> B{G is the one before, and the hash matches?}
//!     B -- yes --> L[lost reply: end the current pair, give a new one]
//!     B -- no --> A{G is the next one, on a chain not used since the load?}
//!     A -- yes --> M[the saved form was behind: give generation G+1]
//!     A -- no --> O{G is older than the one before?}
//!     O -- yes --> E[reuse: end the sign-in]
//!     O -- no --> U[refuse: not known]
//! ```
//!
//! The store does no I/O and reads no clock. The caller passes `now`.
//!
//! # Saved form
//!
//! [`Tokens::to_bytes`] gives the store as JSON, and [`Tokens::from_bytes`]
//! loads it again (R124). The JSON holds only the sign-ins and the
//! chains, with only the hashes of the refresh tokens (R81). It holds
//! no access token (01M3TFG551C76BP4TRA32P7VC3), and no person: the
//! people are in the log. Each time in it is a
//! wall-clock time, so the time that the server was down counts. The
//! store keeps each time as a deadline in the future, so a new process
//! can always hold it.
//!
//! ```
//! use std::time::{Instant, SystemTime};
//! use riff_server::token::{Refused, Tokens};
//!
//! let (now, wall) = (Instant::now(), SystemTime::now());
//! let mut tokens = Tokens::default();
//! let pair = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
//! let bytes = tokens.to_bytes(now, wall);
//! let secret = pair.refresh_token.rsplit('.').next().unwrap();
//! assert!(!String::from_utf8_lossy(&bytes).contains(secret));
//!
//! // After a load, the refresh token works. The access token does not.
//! let now = Instant::now();
//! let mut loaded = Tokens::from_bytes(&bytes, now, SystemTime::now()).unwrap();
//! assert_eq!(loaded.check(&pair.access_token, "k", now), Err(Refused::Unknown));
//! let next = loaded.refresh(&pair.refresh_token, "k", now).unwrap();
//! assert_eq!(loaded.check(&next.access_token, "k", now).unwrap(), "mike");
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
//! let first = tokens.sign_in("mike@comotechnologies.io", "jkt-laptop", now).unwrap();
//! assert_eq!(tokens.check(&first.access_token, "jkt-laptop", now).unwrap(), "mike");
//!
//! // Another device key cannot use the token.
//! assert_eq!(tokens.check(&first.access_token, "jkt-thief", now), Err(Refused::WrongKey));
//!
//! // A refresh token names its chain and its generation.
//! let part = |token: &str, n: usize| token.split('.').nth(n).unwrap().to_owned();
//! assert_eq!(part(&first.refresh_token, 1), "1");
//!
//! // A refresh token gives the next generation of its chain.
//! let second = tokens.refresh(&first.refresh_token, "jkt-laptop", now).unwrap();
//! assert_eq!(part(&second.refresh_token, 0), part(&first.refresh_token, 0));
//! assert_eq!(part(&second.refresh_token, 1), "2");
//! assert_eq!(tokens.check(&second.access_token, "jkt-laptop", now).unwrap(), "mike");
//!
//! // A session token acts only as its session. It has no refresh token.
//! let session = tokens.for_session(&second.access_token, "jkt-laptop", "a6cf", now).unwrap();
//! let who = tokens.caller(&session.access_token, "jkt-laptop", now).unwrap();
//! assert_eq!(who.to_string(), "mike/a6cf");
//! assert_eq!(session.refresh_token, "");
//!
//! // A second use before the next refresh is a lost reply: it ends the
//! // pair of the first use and gives a new one.
//! let again = tokens.refresh(&first.refresh_token, "jkt-laptop", now).unwrap();
//! assert_eq!(tokens.check(&second.access_token, "jkt-laptop", now), Err(Refused::Unknown));
//!
//! // A use after the next refresh revokes the sign-in.
//! tokens.refresh(&again.refresh_token, "jkt-laptop", now).unwrap();
//! assert_eq!(tokens.refresh(&first.refresh_token, "jkt-laptop", now), Err(Refused::Reused));
//! assert_eq!(tokens.check(&session.access_token, "jkt-laptop", now), Err(Refused::Unknown));
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use riff_core::name::Who;
use riff_core::wire::TokenReply;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::oidc::user_of;

/// An access token works this long (R17).
pub const ACCESS_TTL: Duration = Duration::from_secs(10 * 60);

/// A sign-in ends when no refresh token of it is used this long (R80).
pub const REFRESH_IDLE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The USER of the riff server itself. The server posts its own notes as
/// this USER, so no person signs in with it (01M3N7K4BC1RPZKQ1XNDTBRPGF).
pub const SERVER_USER: &str = "riff";

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

/// Why a sign-in did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoSignIn {
    /// The email gives no valid USER (R208).
    Email(String),
    /// The sign-ins of the USER ended after the start of this sign-in:
    /// a removal or a revoke came while the person signed in
    /// (01M3XA87A9GGFA89RQXWSKY0V6).
    Ended,
}

impl fmt::Display for NoSignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoSignIn::Email(why) => f.write_str(why),
            NoSignIn::Ended => {
                f.write_str("each sign-in of this person ended a moment ago; sign in again")
            }
        }
    }
}

impl std::error::Error for NoSignIn {}

type Hash = [u8; 32];

/// All tokens of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct Tokens {
    sign_ins: HashMap<u64, SignIn>,
    /// The live access tokens. They are only in memory.
    access: HashMap<Hash, Access>,
    /// The chains of refresh tokens, by chain ID.
    chains: HashMap<String, Chain>,
    next_sign_in: u64,
    /// The position of the log of the last end of the sign-ins of each
    /// USER. No sign-in of the USER starts below it
    /// (01M3XA87A9GGFA89RQXWSKY0V6). It is only in memory: a load gets
    /// it from the people of the log ([`Tokens::drop_ended`]).
    ended: HashMap<String, u64>,
}

struct SignIn {
    user: String,
    /// The thumbprint of the device key.
    jkt: String,
    /// The position of the log at the start of the sign-in.
    position: u64,
    /// The sign-in ends at this time, unless a refresh comes first.
    idle_until: Instant,
}

struct Access {
    sign_in: u64,
    /// The session of a session token. `None` for a person token.
    session: Option<String>,
    expires: Instant,
}

/// The chain of refresh tokens of a sign-in (01M3TFG4SJ5C96NH8W7XRXJG6Z).
struct Chain {
    sign_in: u64,
    /// The current generation. The first pair has generation 1.
    generation: u64,
    /// The hash of the refresh token of the current generation.
    hash: Hash,
    /// The hash of the refresh token of the generation before: the token
    /// whose use gave the current one. `None` for the first generation.
    before: Option<Hash>,
    /// The hash of the access token of the current pair, to end it after
    /// a lost reply. It is only in memory.
    access: Option<Hash>,
    /// True from the load until the first refresh: the saved form can be
    /// one generation behind (01M3TFG4WE7CZQ4TCJE2NTC52E).
    loaded: bool,
}

/// A refresh token in its parts. The hash is of the whole token.
struct Presented<'a> {
    chain: &'a str,
    generation: u64,
    hash: Hash,
}

impl<'a> Presented<'a> {
    /// `None` when `token` is not `chain.generation.secret`.
    fn parse(token: &'a str) -> Option<Self> {
        let mut parts = token.split('.');
        let (chain, generation, secret) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || chain.is_empty() || secret.is_empty() {
            return None;
        }
        Some(Presented {
            chain,
            generation: generation.parse().ok()?,
            hash: hash(token),
        })
    }
}

/// What a refresh token is for its chain.
enum Step {
    /// The current generation: give the next one.
    Next,
    /// The generation before, while the current one is not used: the
    /// reply was lost.
    Lost,
    /// The generation after the saved one, on a chain not used since the
    /// load.
    Ahead,
}

impl Tokens {
    /// Starts a sign-in for the USER that the email `email` gives (R208)
    /// on the device key `jkt`, and issues its first pair. It asks the
    /// people nothing: it is for a test, or for a riff with no people.
    /// The server uses [`Tokens::start`], after the command `admit`.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let pair = tokens.sign_in("O'Brien@comotechnologies.io", "k", now).unwrap();
    /// assert_eq!(pair.user, "o-brien");
    /// assert!(tokens.sign_in("not-an-email", "k", now).is_err());
    /// ```
    pub fn sign_in(
        &mut self,
        email: &str,
        jkt: &str,
        now: Instant,
    ) -> Result<TokenReply, NoSignIn> {
        let email = email.trim().to_lowercase();
        let user = user_of(&email).map_err(|e| NoSignIn::Email(e.to_string()))?;
        if user == SERVER_USER {
            return Err(NoSignIn::Email(format!(
                "the user {SERVER_USER} is the riff server; sign in with another email"
            )));
        }
        self.start(&user, jkt, 0, now)
    }

    /// Starts a sign-in of `user` on the device key `jkt`, and issues
    /// its first pair. The caller has checked the person (the command
    /// `admit`) and the proof of the key. `position` is the position of
    /// the log at that check: the sign-in keeps it
    /// (01M3XA87A9GGFA89RQXWSKY0V6).
    ///
    /// It refuses a sign-in that started before the last end of the
    /// sign-ins of `user` ([`Tokens::end`]): the removal or the revoke
    /// came after the check of the person.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{NoSignIn, Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let old = tokens.start("bob", "k", 7, now).unwrap();
    /// // A removal of bob is the record at the position 8.
    /// assert_eq!(tokens.end("bob", 8), 1);
    /// assert_eq!(tokens.check(&old.access_token, "k", now), Err(Refused::Unknown));
    /// // A sign-in that the people checked before the removal does not start.
    /// assert_eq!(tokens.start("bob", "k", 7, now), Err(NoSignIn::Ended));
    /// // A sign-in that the people checked after it starts.
    /// assert!(tokens.start("bob", "k", 8, now).is_ok());
    /// ```
    pub fn start(
        &mut self,
        user: &str,
        jkt: &str,
        position: u64,
        now: Instant,
    ) -> Result<TokenReply, NoSignIn> {
        if self.ended.get(user).is_some_and(|ended| position < *ended) {
            return Err(NoSignIn::Ended);
        }
        self.sweep(now);
        let id = self.next_sign_in;
        self.next_sign_in += 1;
        self.sign_ins.insert(
            id,
            SignIn {
                user: user.to_owned(),
                jkt: jkt.to_owned(),
                position,
                idle_until: now + REFRESH_IDLE,
            },
        );
        Ok(self.start_chain(id, now))
    }

    /// Ends each sign-in of `user` that started before `position` of
    /// the log, and each token of them (R20): the effect of a
    /// `member_removed` or a `signins_ended` record at `position`. In
    /// the same step, it keeps `position` for `user`: from now on,
    /// [`Tokens::start`] refuses a sign-in of `user` below it
    /// (01M3XA87A9GGFA89RQXWSKY0V6). Returns the number of sign-ins
    /// that ended.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.start("bob", "laptop", 3, now).unwrap();
    /// tokens.start("bob", "desktop", 9, now).unwrap();
    /// tokens.start("ada", "k", 1, now).unwrap();
    /// // Only the sign-in of bob from before the position 8 ends.
    /// assert_eq!(tokens.end("bob", 8), 1);
    /// assert_eq!(tokens.keys("bob", now), ["desktop"]);
    /// assert_eq!(tokens.keys("ada", now), ["k"]);
    /// ```
    pub fn end(&mut self, user: &str, position: u64) -> usize {
        let ended = self.ended.entry(user.to_owned()).or_default();
        *ended = (*ended).max(position);
        let ids: Vec<u64> = self
            .sign_ins
            .iter()
            .filter(|(_, s)| s.user == user && s.position < position)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.revoke(*id);
        }
        ids.len()
    }

    /// Drops each sign-in that started before the last end of the
    /// sign-ins of its USER: [`Tokens::end`] for each USER of `ended`,
    /// the positions that the people of the log keep. The server calls
    /// it after a load. So a stop between the write of a removal and
    /// the end of the sign-ins lets no removed person in
    /// (01M3XA87A9GGFA89RQXWSKY0V6). Returns the number of sign-ins
    /// that it dropped.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use std::time::{Instant, SystemTime};
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let (now, wall) = (Instant::now(), SystemTime::now());
    /// let mut tokens = Tokens::default();
    /// let bob = tokens.start("bob", "k", 7, now).unwrap();
    /// // The server stops after the write of the removal at the
    /// // position 8, and before the end of the sign-ins.
    /// let mut loaded = Tokens::from_bytes(&tokens.to_bytes(now, wall), now, wall).unwrap();
    /// assert_eq!(loaded.drop_ended(&BTreeMap::from([("bob".to_owned(), 8)])), 1);
    /// assert_eq!(loaded.refresh(&bob.refresh_token, "k", now), Err(Refused::Unknown));
    /// ```
    pub fn drop_ended(&mut self, ended: &BTreeMap<String, u64>) -> usize {
        ended
            .iter()
            .map(|(user, position)| self.end(user, *position))
            .sum()
    }

    /// True when `token` names a chain of a live sign-in on the device
    /// key `jkt`. It checks no generation and no secret, and it changes
    /// nothing.
    pub fn knows_refresh(&self, token: &str, jkt: &str) -> bool {
        Presented::parse(token)
            .and_then(|t| self.chains.get(t.chain))
            .and_then(|c| self.sign_ins.get(&c.sign_in))
            .is_some_and(|s| s.jkt == jkt)
    }

    /// Swaps a refresh token for the next pair of its chain. It works
    /// only with the device key `jkt` of its sign-in. See the rules and
    /// the diagram in the module docs.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let g1 = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
    /// let g2 = tokens.refresh(&g1.refresh_token, "k", now).unwrap();
    /// let g3 = tokens.refresh(&g2.refresh_token, "k", now).unwrap();
    ///
    /// // Another key does not end the sign-in with an old generation.
    /// assert_eq!(tokens.refresh(&g1.refresh_token, "thief", now), Err(Refused::WrongKey));
    /// assert!(tokens.check(&g3.access_token, "k", now).is_ok());
    ///
    /// // The device key with an old generation ends it.
    /// assert_eq!(tokens.refresh(&g1.refresh_token, "k", now), Err(Refused::Reused));
    /// assert_eq!(tokens.check(&g3.access_token, "k", now), Err(Refused::Unknown));
    /// ```
    pub fn refresh(&mut self, token: &str, jkt: &str, now: Instant) -> Result<TokenReply, Refused> {
        self.sweep(now);
        let presented = Presented::parse(token).ok_or(Refused::Unknown)?;
        let chain = self.chains.get(presented.chain).ok_or(Refused::Unknown)?;
        let id = chain.sign_in;
        let sign_in = self.sign_ins.get_mut(&id).ok_or(Refused::Unknown)?;
        // The device key comes before the generation: only it can end
        // the sign-in by reuse (R110).
        if sign_in.jkt != jkt {
            return Err(Refused::WrongKey);
        }
        let current = chain.generation;
        let step = if presented.generation == current && presented.hash == chain.hash {
            Step::Next
        } else if presented.generation + 1 == current && chain.before == Some(presented.hash) {
            Step::Lost
        } else if presented.generation == current + 1 && chain.loaded {
            Step::Ahead
        } else if presented.generation + 1 < current {
            self.revoke(id);
            return Err(Refused::Reused);
        } else {
            return Err(Refused::Unknown);
        };
        sign_in.idle_until = now + REFRESH_IDLE;
        let (generation, before) = match step {
            Step::Next => (current + 1, Some(chain.hash)),
            // The pair of the lost reply ends. The new pair takes its
            // generation, so the token of the lost reply is not known.
            Step::Lost => (current, chain.before),
            Step::Ahead => (presented.generation + 1, Some(presented.hash)),
        };
        if let (Step::Lost, Some(lost)) = (&step, chain.access) {
            self.access.remove(&lost);
        }
        let chain_id = presented.chain.to_owned();
        Ok(self.issue(&chain_id, generation, before, now))
    }

    /// Swaps a live person access token for a session access token in
    /// the same sign-in (R19). The session token works only for
    /// `session`, and only with the device key `jkt`. The reply has no
    /// refresh token, and the swap ends no other token: each process of
    /// the session keeps its own (01M3WFVAB44T8EP4QZD4KS7DRF).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let person = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
    /// let long = tokens.for_session(&person.access_token, "k", "a6cf", now).unwrap();
    /// let short = tokens.for_session(&person.access_token, "k", "a6cf", now).unwrap();
    /// for token in [&long, &short] {
    ///     let who = tokens.caller(&token.access_token, "k", now).unwrap();
    ///     assert_eq!(who.to_string(), "mike/a6cf");
    ///     assert_eq!(token.refresh_token, "");
    /// }
    /// assert_eq!(tokens.chains(), 1);
    ///
    /// // The end of the sign-in ends each session token.
    /// tokens.revoke_user("mike");
    /// assert_eq!(tokens.caller(&long.access_token, "k", now), Err(Refused::Unknown));
    /// ```
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
        let access_token = self.issue_access(id, Some(session.to_owned()), now);
        Ok(TokenReply {
            access_token,
            token_type: "DPoP".into(),
            expires_in: ACCESS_TTL.as_secs(),
            refresh_token: String::new(),
            user: who.user().to_owned(),
        })
    }

    /// The number of live chains: one for each sign-in.
    pub fn chains(&self) -> usize {
        self.chains.len()
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

    /// Ends each sign-in of `user` and each token of them, with no
    /// position: for a test. The server uses [`Tokens::end`]. Returns
    /// the number of sign-ins that ended.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let laptop = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
    /// let desktop = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
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

    /// The thumbprints of the device keys of the live sign-ins of
    /// `user`, sorted, each once. A reader checks the signature of a
    /// message from `user` against them (R199).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{REFRESH_IDLE, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.sign_in("mike@comotechnologies.io", "laptop", now).unwrap();
    /// tokens.sign_in("mike@comotechnologies.io", "desktop", now).unwrap();
    /// tokens.sign_in("mike@comotechnologies.io", "laptop", now).unwrap();
    /// tokens.sign_in("brett@comotechnologies.io", "heron", now).unwrap();
    /// assert_eq!(tokens.keys("mike", now), ["desktop", "laptop"]);
    /// assert!(tokens.keys("mike", now + REFRESH_IDLE).is_empty());
    /// tokens.revoke_user("brett");
    /// assert!(tokens.keys("brett", now).is_empty());
    /// ```
    pub fn keys(&self, user: &str, now: Instant) -> Vec<String> {
        let keys: BTreeSet<&str> = self
            .sign_ins
            .values()
            .filter(|s| s.user == user && now < s.idle_until)
            .map(|s| s.jkt.as_str())
            .collect();
        keys.into_iter().map(str::to_owned).collect()
    }

    /// The store as JSON, with only the hashes of the refresh tokens, and
    /// with no access token (R81, 01M3TFG551C76BP4TRA32P7VC3). `now` and
    /// `wall` are the same time on the two clocks.
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
                        position: s.position,
                        idle_until: live(s.idle_until)?,
                    })
                })
                .collect(),
            chains: self
                .chains
                .iter()
                .map(|(id, c)| SavedChain {
                    id: id.clone(),
                    sign_in: c.sign_in,
                    generation: c.generation,
                    hash: URL_SAFE_NO_PAD.encode(c.hash),
                    before: c.before.map(|h| URL_SAFE_NO_PAD.encode(h)),
                })
                .collect(),
        };
        serde_json::to_vec(&saved).expect("the saved form is JSON")
    }

    /// Loads a store from [`Tokens::to_bytes`]. It drops each sign-in
    /// that ended while the server was down, with its chain. The first refresh
    /// of each loaded chain takes the next generation as good too
    /// (01M3TFG4WE7CZQ4TCJE2NTC52E).
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
                    position: s.position,
                    idle_until,
                };
                tokens.sign_ins.insert(s.id, sign_in);
            }
        }
        for c in saved.chains {
            let chain = Chain {
                sign_in: c.sign_in,
                generation: c.generation,
                hash: unhash(&c.hash)?,
                before: c.before.as_deref().map(unhash).transpose()?,
                access: None,
                loaded: true,
            };
            tokens.chains.insert(c.id, chain);
        }
        tokens.sweep(now);
        Ok(tokens)
    }

    /// Starts the chain of `sign_in` and gives its first pair.
    fn start_chain(&mut self, sign_in: u64, now: Instant) -> TokenReply {
        let id = random_id();
        self.chains.insert(
            id.clone(),
            Chain {
                sign_in,
                generation: 0,
                hash: Hash::default(),
                before: None,
                access: None,
                loaded: false,
            },
        );
        self.issue(&id, 1, None, now)
    }

    /// Gives the person pair of `generation` of a chain. `before` is the
    /// hash of the refresh token of the generation before it.
    fn issue(
        &mut self,
        chain_id: &str,
        generation: u64,
        before: Option<Hash>,
        now: Instant,
    ) -> TokenReply {
        let refresh_token = format!("{chain_id}.{generation}.{}", random_token());
        let Some(sign_in) = self.chains.get(chain_id).map(|c| c.sign_in) else {
            unreachable!("the caller found the chain")
        };
        let access_token = self.issue_access(sign_in, None, now);
        if let Some(chain) = self.chains.get_mut(chain_id) {
            chain.generation = generation;
            chain.hash = hash(&refresh_token);
            chain.before = before;
            chain.access = Some(hash(&access_token));
            chain.loaded = false;
        }
        let user = self
            .sign_ins
            .get(&sign_in)
            .map_or_else(String::new, |s| s.user.clone());
        TokenReply {
            access_token,
            token_type: "DPoP".into(),
            expires_in: ACCESS_TTL.as_secs(),
            refresh_token,
            user,
        }
    }

    /// Gives a new access token of `sign_in`. `session` is `None` for a
    /// person token.
    fn issue_access(&mut self, sign_in: u64, session: Option<String>, now: Instant) -> String {
        let access_token = random_token();
        self.access.insert(
            hash(&access_token),
            Access {
                sign_in,
                session,
                expires: now + ACCESS_TTL,
            },
        );
        access_token
    }

    /// Ends one sign-in and each token of it.
    fn revoke(&mut self, sign_in: u64) {
        self.sign_ins.remove(&sign_in);
        self.access.retain(|_, a| a.sign_in != sign_in);
        self.chains.retain(|_, c| c.sign_in != sign_in);
    }

    /// Forgets expired access tokens, and idle sign-ins with their
    /// chains.
    fn sweep(&mut self, now: Instant) {
        self.sign_ins.retain(|_, s| now < s.idle_until);
        let live = &self.sign_ins;
        self.access
            .retain(|_, a| now < a.expires && live.contains_key(&a.sign_in));
        self.chains.retain(|_, c| live.contains_key(&c.sign_in));
    }
}

/// Why [`Tokens::from_bytes`] failed. The text does not name the
/// object: [`crate::store::StoreError::NotValid`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError(String);

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LoadError {}

/// The saved form: only the sign-ins and their chains. Each time is in
/// milliseconds since the Unix epoch. A form from before the people
/// moved to the log reads: its other fields are skipped.
#[derive(Serialize, Deserialize)]
struct Saved {
    next_sign_in: u64,
    sign_ins: Vec<SavedSignIn>,
    chains: Vec<SavedChain>,
}

#[derive(Serialize, Deserialize)]
struct SavedSignIn {
    id: u64,
    user: String,
    jkt: String,
    /// The position of the log at the start of the sign-in
    /// (01M3XA87A9GGFA89RQXWSKY0V6). A form from before it has 0.
    #[serde(default)]
    position: u64,
    idle_until: u64,
}

/// One chain: the hash of the refresh token of its current generation,
/// and of the one before it.
#[derive(Serialize, Deserialize)]
struct SavedChain {
    id: String,
    sign_in: u64,
    generation: u64,
    hash: String,
    before: Option<String>,
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

/// A new chain ID: 16 random bytes in URL-safe base64.
fn random_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A new random token: 32 random bytes in URL-safe base64. The ID of a
/// new riff is one too.
pub(crate) fn random_token() -> String {
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
        let pair = tokens
            .sign_in("mike@comotechnologies.io", "k", now)
            .unwrap();
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
        let third = tokens.refresh(&second.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        assert_eq!(
            tokens.check(&third.access_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&third.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    #[test]
    fn a_lost_reply_gives_a_new_pair_and_ends_the_lost_one() {
        let (mut tokens, first, now) = signed_in();
        // The reply with this pair never came back.
        let lost = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let again = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.check(&again.access_token, "k", now),
            Ok("mike".to_owned())
        );
        assert_eq!(
            tokens.check(&lost.access_token, "k", now),
            Err(Refused::Unknown)
        );
        assert_eq!(
            tokens.refresh(&lost.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
        // Each lost reply counts, until the next refresh.
        let third = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.check(&again.access_token, "k", now),
            Err(Refused::Unknown)
        );
        let fourth = tokens.refresh(&third.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        assert_eq!(
            tokens.check(&fourth.access_token, "k", now),
            Err(Refused::Unknown)
        );
    }

    /// A lost reply ends the person access token of the lost pair.
    /// The session tokens of the sign-in stay.
    #[test]
    fn a_lost_reply_of_a_person_refresh_keeps_the_session_tokens() {
        let (mut tokens, person, now) = signed_in();
        let session = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        tokens.refresh(&person.refresh_token, "k", now).unwrap();
        tokens.refresh(&person.refresh_token, "k", now).unwrap();
        let who = tokens.caller(&session.access_token, "k", now).unwrap();
        assert_eq!(who.to_string(), "mike/a");
    }

    #[test]
    fn a_lost_reply_still_counts_after_a_restart() {
        let (mut tokens, first, now) = signed_in();
        let lost = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        let again = loaded.refresh(&first.refresh_token, "k", now).unwrap();
        assert_eq!(
            loaded.check(&lost.access_token, "k", now),
            Err(Refused::Unknown)
        );
        loaded.refresh(&again.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&loaded, now, Duration::from_secs(5));
        assert_eq!(
            loaded.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
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
        let other = tokens
            .sign_in("mike@comotechnologies.io", "k", now)
            .unwrap();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&second.refresh_token, "k", now).unwrap();
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
        let brett = tokens
            .sign_in("brett@comotechnologies.io", "k", now)
            .unwrap();
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
        let again = tokens
            .sign_in("mike@comotechnologies.io", "k", now)
            .unwrap();
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
        tokens
            .sign_in("brett@comotechnologies.io", "k", now + ACCESS_TTL)
            .unwrap();
        assert_eq!(tokens.access.len(), 1);
        assert_eq!(tokens.sign_ins.len(), 2);
    }

    /// The parts of a refresh token: the chain, the generation and the
    /// secret.
    fn parts(token: &str) -> (String, u64, String) {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "{token}");
        (parts[0].into(), parts[1].parse().unwrap(), parts[2].into())
    }

    #[test]
    fn a_refresh_token_names_its_chain_and_its_generation() {
        let (mut tokens, first, now) = signed_in();
        let (chain, generation, secret) = parts(&first.refresh_token);
        assert_eq!(generation, 1);
        assert_eq!(secret.len(), 43);
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (same, next, other) = parts(&second.refresh_token);
        assert_eq!((same, next), (chain, 2));
        assert_ne!(other, secret);
    }

    #[test]
    fn the_server_keeps_only_the_hash_of_the_current_generation_and_the_one_before() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let third = tokens.refresh(&second.refresh_token, "k", now).unwrap();
        assert_eq!(tokens.chains(), 1);
        let text = String::from_utf8(tokens.to_bytes(now, SystemTime::now())).unwrap();
        let saved = |token: &str| text.contains(&URL_SAFE_NO_PAD.encode(hash(token)));
        assert!(saved(&third.refresh_token), "{text}");
        assert!(saved(&second.refresh_token), "{text}");
        assert!(!saved(&first.refresh_token), "{text}");
        assert!(text.contains(r#""generation":3"#), "{text}");
    }

    #[test]
    fn an_older_generation_is_reuse_at_any_time() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let later = now + REFRESH_IDLE / 2;
        let third = tokens.refresh(&second.refresh_token, "k", later).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", later),
            Err(Refused::Reused)
        );
        assert_eq!(
            tokens.refresh(&third.refresh_token, "k", later),
            Err(Refused::Unknown)
        );
        assert_eq!(tokens.chains(), 0);
    }

    #[test]
    fn the_key_is_checked_before_the_generation() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let third = tokens.refresh(&second.refresh_token, "k", now).unwrap();
        // An old generation, and a generation that the server never
        // gave, with another key: refused, and the sign-in stays.
        let (chain, _, _) = parts(&first.refresh_token);
        for token in [first.refresh_token.clone(), format!("{chain}.0.x")] {
            assert_eq!(tokens.refresh(&token, "thief", now), Err(Refused::WrongKey));
        }
        assert_eq!(
            tokens.check(&third.access_token, "k", now),
            Ok("mike".to_owned())
        );
        assert!(tokens.refresh(&third.refresh_token, "k", now).is_ok());
    }

    #[test]
    fn a_wrong_secret_is_not_known_and_changes_nothing() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (chain, _, _) = parts(&second.refresh_token);
        // The current generation, the one before and the next one.
        for generation in [2, 1, 3] {
            let forged = format!("{chain}.{generation}.{}", random_token());
            assert_eq!(
                tokens.refresh(&forged, "k", now),
                Err(Refused::Unknown),
                "generation {generation}"
            );
        }
        for token in ["a.b", "a.1.b.c", ".1.b", "a.1.", "a.x.b", "a.-1.b"] {
            assert_eq!(tokens.refresh(token, "k", now), Err(Refused::Unknown));
        }
        assert_eq!(
            tokens.check(&second.access_token, "k", now),
            Ok("mike".to_owned())
        );
        assert!(tokens.refresh(&second.refresh_token, "k", now).is_ok());
    }

    /// The fault of #381: a new token for a session ended the token of
    /// each other process of that session.
    #[test]
    fn a_new_session_token_ends_no_other_token() {
        let (mut tokens, person, now) = signed_in();
        let long = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        let other = tokens
            .for_session(&person.access_token, "k", "b", now)
            .unwrap();
        for _ in 0..10 {
            let short = tokens
                .for_session(&person.access_token, "k", "a", now)
                .unwrap();
            assert_ne!(short.access_token, long.access_token);
            let who = |t: &str| tokens.caller(t, "k", now).unwrap().to_string();
            assert_eq!(who(&short.access_token), "mike/a");
            assert_eq!(who(&long.access_token), "mike/a");
            assert_eq!(who(&other.access_token), "mike/b");
            assert_eq!(who(&person.access_token), "mike");
        }
    }

    #[test]
    fn a_session_token_has_no_refresh_token_and_no_chain() {
        let (mut tokens, person, now) = signed_in();
        let a = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        assert_eq!(a.refresh_token, "");
        assert_eq!(a.expires_in, ACCESS_TTL.as_secs());
        assert_eq!(tokens.chains(), 1, "only the person chain");
        assert_eq!(tokens.refresh("", "k", now), Err(Refused::Unknown));
        // A session token is no refresh token.
        assert_eq!(
            tokens.refresh(&a.access_token, "k", now),
            Err(Refused::Unknown)
        );
        // The saved form holds nothing of the session.
        let text = String::from_utf8(tokens.to_bytes(now, SystemTime::now())).unwrap();
        assert!(!text.contains("session"), "{text}");
        // A second sign-in of the person has its own chain.
        let other = tokens
            .sign_in("mike@comotechnologies.io", "k", now)
            .unwrap();
        tokens
            .for_session(&other.access_token, "k", "a", now)
            .unwrap();
        assert_eq!(tokens.chains(), 2);
    }

    #[test]
    fn a_session_token_expires_and_the_person_token_gives_a_new_one() {
        let (mut tokens, person, now) = signed_in();
        let a = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        let later = now + ACCESS_TTL;
        assert_eq!(
            tokens.caller(&a.access_token, "k", later),
            Err(Refused::Expired)
        );
        let person = tokens.refresh(&person.refresh_token, "k", later).unwrap();
        let again = tokens
            .for_session(&person.access_token, "k", "a", later)
            .unwrap();
        let who = tokens.caller(&again.access_token, "k", later).unwrap();
        assert_eq!(who.to_string(), "mike/a");
        // The sweep of the swap forgot the expired token.
        assert_eq!(
            tokens.caller(&a.access_token, "k", later),
            Err(Refused::Unknown)
        );
    }

    /// Each end of a sign-in ends its session tokens at once (R20): a
    /// revoke of the person, a removed member, and a reused refresh
    /// token.
    #[test]
    fn the_end_of_a_sign_in_ends_each_session_token_at_once() {
        let now = Instant::now();
        let session = |tokens: &mut Tokens, person: &TokenReply, key: &str| {
            let a = tokens
                .for_session(&person.access_token, key, "a", now)
                .unwrap();
            let b = tokens
                .for_session(&person.access_token, key, "a", now)
                .unwrap();
            assert!(tokens.caller(&a.access_token, key, now).is_ok());
            [a.access_token, b.access_token]
        };
        let gone = |tokens: &Tokens, held: &[String], key: &str| {
            for token in held {
                assert_eq!(tokens.caller(token, key, now), Err(Refused::Unknown));
            }
        };

        // A revoke of the person.
        let (mut tokens, person, _) = signed_in();
        let held = session(&mut tokens, &person, "k");
        assert_eq!(tokens.revoke_user("mike"), 1);
        gone(&tokens, &held, "k");
        assert_eq!(
            tokens.for_session(&person.access_token, "k", "a", now),
            Err(Refused::Unknown)
        );

        // A removed member.
        let mut tokens = Tokens::default();
        tokens.start("ada", "k1", 1, now).unwrap();
        let bob = tokens.start("bob", "k2", 3, now).unwrap();
        let held = session(&mut tokens, &bob, "k2");
        assert_eq!(tokens.end("bob", 4), 1);
        gone(&tokens, &held, "k2");

        // A reused refresh token.
        let (mut tokens, first, _) = signed_in();
        let held = session(&mut tokens, &first, "k");
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&second.refresh_token, "k", now).unwrap();
        assert_eq!(
            tokens.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        gone(&tokens, &held, "k");
    }

    /// The server stopped after a refresh and before it saved the
    /// sign-ins. The saved form is one generation behind, and the
    /// sign-in stays (01M3TFG4WE7CZQ4TCJE2NTC52E).
    #[test]
    fn a_crash_after_a_refresh_and_before_the_save_does_not_end_the_sign_in() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let wall = SystemTime::now();
        let saved = tokens.to_bytes(now, wall);
        // The client has the third pair. The saved form has the second.
        let third = tokens.refresh(&second.refresh_token, "k", now).unwrap();
        drop(tokens);

        let mut loaded = Tokens::from_bytes(&saved, now, wall).unwrap();
        let fourth = loaded.refresh(&third.refresh_token, "k", now).unwrap();
        assert_eq!(parts(&fourth.refresh_token).1, 4);
        assert_eq!(
            loaded.check(&fourth.access_token, "k", now),
            Ok("mike".to_owned())
        );
        // The chain goes on as before the crash: a lost reply, the next
        // refresh, and a reuse.
        let again = loaded.refresh(&third.refresh_token, "k", now).unwrap();
        loaded.refresh(&again.refresh_token, "k", now).unwrap();
        assert_eq!(
            loaded.refresh(&second.refresh_token, "k", now),
            Err(Refused::Reused)
        );
    }

    #[test]
    fn after_a_load_the_current_generation_is_good() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        let third = loaded.refresh(&second.refresh_token, "k", now).unwrap();
        assert_eq!(parts(&third.refresh_token).1, 3);
    }

    #[test]
    fn the_next_generation_is_good_only_for_the_first_refresh_after_a_load() {
        let (mut tokens, first, now) = signed_in();
        let (chain, _, _) = parts(&first.refresh_token);
        let next = |generation: u64| format!("{chain}.{generation}.{}", random_token());
        // On a chain that the server did not load, the next generation is
        // not known.
        assert_eq!(tokens.refresh(&next(2), "k", now), Err(Refused::Unknown));

        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        // Two generations after the saved one: not known.
        assert_eq!(loaded.refresh(&next(3), "k", now), Err(Refused::Unknown));
        // Another key: refused.
        assert_eq!(
            loaded.refresh(&next(2), "thief", now),
            Err(Refused::WrongKey)
        );
        let third = loaded.refresh(&next(2), "k", now).unwrap();
        assert_eq!(parts(&third.refresh_token).1, 3);
        // The chain was used since the load: the next generation is not
        // known again.
        assert_eq!(loaded.refresh(&next(4), "k", now), Err(Refused::Unknown));
        assert!(loaded.refresh(&third.refresh_token, "k", now).is_ok());
    }

    #[test]
    fn a_refresh_with_the_current_generation_ends_the_rule_for_the_next_one() {
        let (tokens, first, now) = signed_in();
        let (chain, _, _) = parts(&first.refresh_token);
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        let second = loaded.refresh(&first.refresh_token, "k", now).unwrap();
        let forged = format!("{chain}.3.{}", random_token());
        assert_eq!(loaded.refresh(&forged, "k", now), Err(Refused::Unknown));
        assert!(loaded.refresh(&second.refresh_token, "k", now).is_ok());
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
    fn bad_emails_are_refused() {
        let mut tokens = Tokens::default();
        assert!(tokens.sign_in("", "k", Instant::now()).is_err());
        assert!(tokens.sign_in("not-an-email", "k", Instant::now()).is_err());
        assert!(tokens.sign_in("@x.io", "k", Instant::now()).is_err());
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
    fn a_session_token_acts_only_as_its_session() {
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
    fn only_a_person_token_gives_a_session_token() {
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
    fn nobody_signs_in_as_the_riff_server() {
        let mut tokens = Tokens::default();
        let refused = tokens.sign_in("Riff@gmail.com", "k", Instant::now());
        assert!(
            matches!(&refused, Err(NoSignIn::Email(why)) if why.contains("the riff server")),
            "{refused:?}"
        );
    }

    #[test]
    fn refresh_tokens_work_after_a_restart_and_access_tokens_do_not() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let session = tokens
            .for_session(&second.access_token, "k", "a", now)
            .unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        for token in [&second.access_token, &session.access_token] {
            assert_eq!(loaded.check(token, "k", now), Err(Refused::Unknown));
        }
        let who = |t: &Tokens, token: &str| t.caller(token, "k", now).unwrap().to_string();
        let third = loaded.refresh(&second.refresh_token, "k", now).unwrap();
        assert_eq!(who(&loaded, &third.access_token), "mike");
        // The new person token gives the session a new token.
        let next = loaded
            .for_session(&third.access_token, "k", "a", now)
            .unwrap();
        assert_eq!(who(&loaded, &next.access_token), "mike/a");
        assert_eq!(
            loaded.check(&third.access_token, "thief", now),
            Err(Refused::WrongKey)
        );
    }

    #[test]
    fn a_used_refresh_token_still_revokes_after_a_restart() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let third = tokens.refresh(&second.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        assert_eq!(
            loaded.refresh(&first.refresh_token, "k", now),
            Err(Refused::Reused)
        );
        assert_eq!(
            loaded.check(&third.access_token, "k", now),
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
            let secret = token.rsplit('.').next().unwrap();
            assert!(!text.contains(secret), "{text}");
        }
        // It holds the hash of the refresh token, and no access token.
        let saved = |token: &str| text.contains(&URL_SAFE_NO_PAD.encode(hash(token)));
        assert!(saved(&second.refresh_token), "{text}");
        assert!(!saved(&second.access_token), "{text}");
    }

    #[test]
    fn the_time_that_the_server_was_down_counts() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&second.refresh_token, "k", now).unwrap();
        let (mut loaded, later) = restart(&tokens, now, ACCESS_TTL);
        assert_eq!(
            loaded.check(&first.access_token, "k", later),
            Err(Refused::Unknown)
        );
        assert_eq!(loaded.access.len(), 0);
        // The old generation is still a reuse.
        assert_eq!(
            loaded.refresh(&first.refresh_token, "k", later),
            Err(Refused::Reused)
        );
    }

    #[test]
    fn a_restart_forgets_what_ended_while_the_server_was_down() {
        let (mut tokens, person, now) = signed_in();
        tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        let (loaded, _) = restart(&tokens, now, REFRESH_IDLE - Duration::from_secs(1));
        assert_eq!(loaded.chains(), 1, "the person chain");
        let (loaded, _) = restart(&tokens, now, REFRESH_IDLE);
        assert!(loaded.sign_ins.is_empty() && loaded.chains.is_empty());
    }

    #[test]
    fn a_new_sign_in_after_a_restart_gets_a_new_id() {
        let (mut tokens, first, now) = signed_in();
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&second.refresh_token, "k", now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::ZERO);
        let other = loaded
            .sign_in("mike@comotechnologies.io", "k", now)
            .unwrap();
        loaded.refresh(&first.refresh_token, "k", now).unwrap_err();
        assert_eq!(
            loaded.check(&other.access_token, "k", now),
            Ok("mike".to_owned())
        );
    }

    #[test]
    fn a_sign_in_keeps_its_position_over_a_restart() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        let early = tokens.start("bob", "k", 3, now).unwrap();
        let late = tokens.start("bob", "k", 9, now).unwrap();
        let (mut loaded, now) = restart(&tokens, now, Duration::from_secs(5));
        // The removal of bob is the record at the position 8.
        assert_eq!(
            loaded.drop_ended(&BTreeMap::from([("bob".to_owned(), 8)])),
            1
        );
        assert_eq!(
            loaded.refresh(&early.refresh_token, "k", now),
            Err(Refused::Unknown)
        );
        assert!(loaded.refresh(&late.refresh_token, "k", now).is_ok());
        // After the load, a sign-in from before the removal does not start.
        assert_eq!(loaded.start("bob", "k", 7, now), Err(NoSignIn::Ended));
    }

    #[test]
    fn an_end_at_an_older_position_does_not_lower_the_position_of_the_user() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        assert_eq!(tokens.end("bob", 8), 0);
        assert_eq!(tokens.end("bob", 5), 0);
        assert_eq!(tokens.start("bob", "k", 7, now), Err(NoSignIn::Ended));
        // Another person is not ended.
        assert!(tokens.start("ada", "k", 0, now).is_ok());
    }

    #[test]
    fn a_bad_saved_form_does_not_load() {
        let (now, wall) = (Instant::now(), SystemTime::now());
        assert!(Tokens::from_bytes(b"{", now, wall).is_err());
        let empty = br#"{"next_sign_in":1,"sign_ins":[],"chains":[]}"#;
        assert!(Tokens::from_bytes(empty, now, wall).is_ok());
        // A form from before the people moved to the log reads.
        let old = br#"{"next_sign_in":1,"users":{"mike":"mike@x.io"},"owner":"mike@x.io","sign_ins":[],"chains":[]}"#;
        assert!(Tokens::from_bytes(old, now, wall).is_ok());
        let bad_hash = br#"{"next_sign_in":1,"sign_ins":[],"chains":[
            {"id":"c","sign_in":0,"generation":1,"hash":"abc","before":null}]}"#;
        assert!(Tokens::from_bytes(bad_hash, now, wall).is_err());
        let bad_id = br#"{"next_sign_in":0,"sign_ins":[
            {"id":0,"user":"mike","jkt":"k","idle_until":18446744073709551615}],
            "chains":[]}"#;
        assert!(Tokens::from_bytes(bad_id, now, wall).is_err());
    }
}
