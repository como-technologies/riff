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
//! Each pair of tokens belongs to one sign-in. A person signs in with a
//! verified email. The USER comes from the email (R208), and the store
//! keeps the email of each USER: the first email that signs in with a
//! USER holds it (R209, see [`Tokens::sign_in`]).
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
//! - A lost reply is not a reuse (01M3MX4TG7PNNETZ986DQS10JJ). When the
//!   refresh token of the pair that the last use gave is still unused,
//!   the reply of that use never came back: for example a 503 after a
//!   failed save, or a process that stopped. The server then ends that
//!   pair and gives a new one. Only a reuse after the next refresh
//!   revokes the sign-in.
//! - A sign-in expires when no refresh token of it is used for
//!   [`REFRESH_IDLE`].
//! - A refresh gives a pair of the same kind: a session pair stays for
//!   its session.
//! - Only a person access token gives a session pair (R105).
//! - [`Tokens::revoke_user`] ends each sign-in of one person at once
//!   (R20). This ends the session pairs too. It leaves the USER of the
//!   person: only that email signs in as that USER again.
//! - A token that the server does not know is refused.
//!
//! The store does no I/O and reads no clock. The caller passes `now`.
//!
//! # Riff ID
//!
//! Each store has a riff ID: random, made with the store
//! (01M3JNVBPMZ1K9WX7Q7DP6Y0DH). It is in the saved form, so a restart
//! on the same store keeps it. A new store is a new riff with a new ID.
//! `riff` keeps the ID with its sign-in, and so finds a sign-in of a
//! riff that is gone.
//!
//! ```
//! use std::time::{Instant, SystemTime};
//! use riff_server::token::Tokens;
//!
//! let (now, wall) = (Instant::now(), SystemTime::now());
//! let tokens = Tokens::default();
//! assert_ne!(tokens.riff_id(), Tokens::default().riff_id());
//! let loaded = Tokens::from_bytes(&tokens.to_bytes(now, wall), now, wall).unwrap();
//! assert_eq!(loaded.riff_id(), tokens.riff_id());
//! ```
//!
//! # Owner and members
//!
//! The store also keeps who may join the riff, by verified email in
//! lower case. [`Tokens::admit`] is the sign-in of a person from the
//! provider. It lets a person in when one of these is true:
//!
//! - The person is the owner, a member or an admin.
//! - The account is in an allowed domain (R15).
//! - The riff is new, with no owner and no admin: the person becomes
//!   the owner. A riff whose owner was gone is not new.
//!
//! The first person that [`Tokens::admit`] lets in is the owner
//! (01M3JN3AD44CC98AGMVP43F56G). On a riff with admins, only an admin
//! becomes the owner. The owner is an admin. An admin adds a member
//! with [`Tokens::invite`] and removes one with [`Tokens::remove`]. A
//! removal ends each sign-in of that person (R20). The owner makes a
//! person an admin with [`Tokens::add_admin`], and a member again with
//! [`Tokens::remove_admin`]. The store keeps these admins; the admins
//! of the settings (R210) add to them. An admin that the owner made
//! is removed only after the owner takes the role back. The owner
//! passes the owner role to a member or an admin with
//! [`Tokens::pass_owner`]; the old owner stays an admin. A riff has one
//! owner at a time.
//!
//! An admin asks for the owner role with [`Tokens::take_owner`]. The
//! request waits for the owner: [`Tokens::pass_owner`] and
//! [`Tokens::deny_owner`] answer it, and [`Tokens::owner_due`] grants it
//! when its time ends. [`Tokens::owner_gone`] ends the role of an owner
//! who is gone. The riff then has no owner: a sign-in and
//! [`Tokens::name_owner`] make none, and the next request makes its
//! admin the owner at once. [`crate::owner`] has the times and the
//! checks of the owner.
//!
//! ```mermaid
//! flowchart TD
//!     A[verified email] --> O{owner, member or admin?}
//!     O -- yes --> IN[sign in]
//!     O -- no --> D{allowed domain?}
//!     D -- yes --> IN
//!     D -- no --> N{no owner and no admin?}
//!     N -- yes --> IN
//!     N -- no --> R[refuse: ask the owner for an invite]
//!     IN --> F{no owner yet, and an admin or no admins?}
//!     F -- yes --> OW[the person is the owner]
//! ```
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
//! let pair = tokens.sign_in("mike@comotechnologies.io", "k", now).unwrap();
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
//! let first = tokens.sign_in("mike@comotechnologies.io", "jkt-laptop", now).unwrap();
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
use riff_core::wire::{RiffOwner, TokenReply};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::oidc::user_of;

/// An access token works this long (R17).
pub const ACCESS_TTL: Duration = Duration::from_secs(10 * 60);

/// A sign-in ends when no refresh token of it is used this long (R80).
pub const REFRESH_IDLE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The server keeps a used refresh token this long, to find reuse
/// (R116).
pub const REUSE_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// The USER of the riff server itself. The server posts its own notes as
/// this USER, so no person signs in with it (01M3N7K4BC1RPZKQ1XNDTBRPGF).
pub const SERVER_USER: &str = "riff";

/// The refusal of an action of the owner on a riff with no owner
/// (01M3N7K48XQ8XSP7R0HD535ZX3).
pub const NO_OWNER: &str =
    "the riff has no owner; an admin takes the owner role with: riff owner --take";

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
    /// Another email holds the USER (R209). It names the USER.
    Taken(String),
    /// The person may not join the riff. It names the email.
    NotMember(String),
}

impl fmt::Display for NoSignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoSignIn::Email(why) => f.write_str(why),
            NoSignIn::Taken(user) => write!(
                f,
                "the user {user} belongs to another account; ask an admin"
            ),
            NoSignIn::NotMember(email) => write!(
                f,
                "{email} is not a member of this riff; ask its owner to run: riff invite {email}"
            ),
        }
    }
}

impl std::error::Error for NoSignIn {}

type Hash = [u8; 32];

/// All tokens of one `riff-server`. See the module docs for the rules.
#[derive(Default)]
pub struct Tokens {
    sign_ins: HashMap<u64, SignIn>,
    access: HashMap<Hash, Access>,
    refresh: HashMap<Hash, Refresh>,
    next_sign_in: u64,
    /// The verified email of each USER, in lower case (R209).
    users: BTreeMap<String, String>,
    /// The email of the owner, in lower case.
    owner: Option<String>,
    /// The email of each member, in lower case.
    members: BTreeSet<String>,
    /// The email of each admin that the owner made, in lower case.
    admins: BTreeSet<String>,
    /// True when the owner was gone and no admin took the role yet
    /// (01M3N7K48XQ8XSP7R0HD535ZX3). A sign-in then makes no owner.
    no_owner: bool,
    /// The request for the owner role that waits for the owner
    /// (01M3N7K3ZAZFGABN7032AYJWEM).
    take: Option<Take>,
    riff_id: RiffId,
}

/// A request for the owner role that waits for the answer of the owner.
#[derive(Clone, Debug)]
struct Take {
    /// The email of the admin that asked, in lower case.
    admin: String,
    /// With no answer before this time, the admin is the owner.
    until: Instant,
}

/// The answer to [`Tokens::take_owner`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Took {
    /// The riff had no owner. The admin is the owner now.
    Owner { owner: String },
    /// The request waits for the answer of `owner`.
    Asked { owner: String, admin: String },
}

/// A change of the owner role that no person made: the server makes it
/// when a time ends (01M3N7K41N03P26BEFFNX5617K,
/// 01M3N7K46H5BRFJCB46P3JNAFZ).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerChange {
    /// The owner `old` did not answer the request in time. The admin
    /// that asked is the `owner` now. `old` stays an admin.
    Granted { owner: String, old: String },
    /// The owner `old` is gone and stays an admin. The admin of a
    /// request that waited is the `owner` now. With no request, the
    /// riff has no owner.
    Gone { old: String, owner: Option<String> },
}

/// The ID of one riff. [`Default`] makes a new, random one.
#[derive(Clone)]
struct RiffId(String);

impl Default for RiffId {
    fn default() -> Self {
        RiffId(random_token())
    }
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
    /// For a used token: the pair that its last use gave.
    gave: Option<Gave>,
}

/// The hashes of the pair that one refresh gave.
#[derive(Clone, Copy)]
struct Gave {
    access: Hash,
    refresh: Hash,
}

impl Tokens {
    /// Starts a sign-in for the verified email `email` on the device key
    /// `jkt`, and issues its first pair. The caller has checked the
    /// email and the proof of the key.
    ///
    /// The USER of the pair comes from the email (R208). The first email
    /// that signs in with a USER holds it. Another email that gives the
    /// same USER gets [`NoSignIn::Taken`] (R209).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{NoSignIn, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// let pair = tokens.sign_in("O'Brien@comotechnologies.io", "k", now).unwrap();
    /// assert_eq!(pair.user, "o-brien");
    ///
    /// // Another email that gives the same USER cannot sign in.
    /// let taken = tokens.sign_in("o-brien@comotechnologies.io", "k", now);
    /// assert_eq!(taken, Err(NoSignIn::Taken("o-brien".into())));
    ///
    /// // The email that holds the USER signs in again, on each device.
    /// assert!(tokens.sign_in("o'brien@comotechnologies.io", "k2", now).is_ok());
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
        match self.users.get(&user) {
            Some(held) if held != &email => return Err(NoSignIn::Taken(user)),
            _ => {
                self.users.insert(user.clone(), email);
            }
        }
        self.sweep(now);
        let id = self.next_sign_in;
        self.next_sign_in += 1;
        self.sign_ins.insert(
            id,
            SignIn {
                user,
                jkt: jkt.to_owned(),
                idle_until: now + REFRESH_IDLE,
            },
        );
        Ok(self.issue(id, None, now))
    }

    /// Signs in a person from the provider, when the person may join the
    /// riff (see "Owner and members" in the module docs).
    /// `allowed_domain` is true when the account is in an allowed domain
    /// (R15). `admins` are the admin emails of the settings (R210).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{NoSignIn, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// // The first person is the owner, from any domain.
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// assert_eq!(tokens.owner(), Some("ada@gmail.com"));
    ///
    /// // A person with no invite and no allowed domain is refused.
    /// let refused = tokens.admit("bob@gmail.com", false, &[], "k2", now);
    /// assert_eq!(refused, Err(NoSignIn::NotMember("bob@gmail.com".into())));
    ///
    /// // After an invite, the person signs in.
    /// tokens.invite("Bob@gmail.com").unwrap();
    /// assert!(tokens.admit("bob@gmail.com", false, &[], "k2", now).is_ok());
    /// ```
    pub fn admit(
        &mut self,
        email: &str,
        allowed_domain: bool,
        admins: &[String],
        jkt: &str,
        now: Instant,
    ) -> Result<TokenReply, NoSignIn> {
        let email = email.trim().to_lowercase();
        let admin =
            self.admins.contains(&email) || admins.iter().any(|a| a.trim().to_lowercase() == email);
        let new_riff = self.owner.is_none() && !self.no_owner && admins.is_empty();
        let may_join = admin
            || allowed_domain
            || new_riff
            || self.owner.as_ref() == Some(&email)
            || self.members.contains(&email);
        if !may_join {
            return Err(NoSignIn::NotMember(email));
        }
        let pair = self.sign_in(&email, jkt, now)?;
        if self.owner.is_none() && !self.no_owner && (admins.is_empty() || admin) {
            self.owner = Some(email);
        }
        Ok(pair)
    }

    /// Names the owner of a new riff, for example from a setting
    /// (01M3JN3ASSV9SA0QZKXXJ0RTEV). A riff that has an owner keeps it.
    /// A riff whose owner was gone keeps no owner
    /// (01M3N7K4GAKJ621V5AWJRQVF3M). Returns the owner.
    ///
    /// ```
    /// use riff_server::token::Tokens;
    ///
    /// let mut tokens = Tokens::default();
    /// assert_eq!(tokens.name_owner(" Ada@X.io"), Some("ada@x.io"));
    /// assert_eq!(tokens.name_owner("bob@x.io"), Some("ada@x.io"));
    /// ```
    pub fn name_owner(&mut self, email: &str) -> Option<&str> {
        if self.owner.is_none() && !self.no_owner {
            self.owner = Some(email.trim().to_lowercase());
        }
        self.owner.as_deref()
    }

    /// True when the riff has an owner, or had one: a riff whose owner
    /// was gone counts (01M3JN3AQMHZHT6JP3P6GM9PWZ).
    pub fn owned(&self) -> bool {
        self.owner.is_some() || self.no_owner
    }

    /// The USER that holds `email`, or `None` when nobody signed in with
    /// it.
    pub fn user_of_email(&self, email: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|(_, held)| *held == email)
            .map(|(user, _)| user.as_str())
    }

    /// The USER of the owner, or `None` when the riff has no owner or the
    /// owner never signed in.
    pub fn owner_user(&self) -> Option<&str> {
        self.user_of_email(self.owner.as_deref()?)
    }

    /// The USER of each admin that is not the owner, sorted: the admins
    /// that the owner made and the admins of `admins` (R210). An admin
    /// who never signed in has no USER, and is not in it.
    pub fn admin_users(&self, admins: &[String]) -> Vec<String> {
        let (_, admins, _) = self.roles(admins);
        self.users
            .iter()
            .filter(|(_, email)| admins.contains(email))
            .map(|(user, _)| user.clone())
            .collect()
    }

    /// An admin asks for the owner role (01M3N7K3ZAZFGABN7032AYJWEM).
    /// `user` is the USER of the caller, `admins` the admin emails of the
    /// settings (R210), and `wait` the time that the owner has to answer
    /// (01M3N7K443DGPZ8XH5WWKK6M35).
    ///
    /// On a riff with no owner, the admin is the owner at once
    /// (01M3N7K48XQ8XSP7R0HD535ZX3). Else the request waits: the owner
    /// answers with [`Tokens::pass_owner`] or [`Tokens::deny_owner`], and
    /// [`Tokens::owner_due`] grants it when `wait` ends. One request
    /// waits at a time: a second request is refused, with the email of
    /// the admin that asked first.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_server::token::{OwnerChange, Took, Tokens};
    ///
    /// let now = Instant::now();
    /// let wait = Duration::from_secs(600);
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// tokens.add_admin("bob@gmail.com").unwrap();
    /// tokens.add_admin("carol@gmail.com").unwrap();
    /// tokens.admit("bob@gmail.com", false, &[], "k2", now).unwrap();
    /// tokens.admit("carol@gmail.com", false, &[], "k3", now).unwrap();
    ///
    /// let asked = tokens.take_owner("bob", &[], wait, now).unwrap();
    /// assert_eq!(
    ///     asked,
    ///     Took::Asked { owner: "ada@gmail.com".into(), admin: "bob@gmail.com".into() }
    /// );
    /// // A second request waits for the first.
    /// let refused = tokens.take_owner("carol", &[], wait, now).unwrap_err();
    /// assert!(refused.contains("bob@gmail.com asked for the owner role first"), "{refused}");
    ///
    /// // With no answer in time, bob is the owner.
    /// assert_eq!(tokens.owner_due(now), None);
    /// assert_eq!(tokens.owner_due(now + wait), Some(OwnerChange::Granted {
    ///     owner: "bob@gmail.com".into(),
    ///     old: "ada@gmail.com".into(),
    /// }));
    /// assert!(tokens.is_owner("bob") && tokens.is_admin("ada", &[]));
    /// ```
    pub fn take_owner(
        &mut self,
        user: &str,
        admins: &[String],
        wait: Duration,
        now: Instant,
    ) -> Result<Took, String> {
        if !self.is_admin(user, admins) {
            return Err(format!(
                "{user} is not an admin; only an admin takes the owner role"
            ));
        }
        // An admin has an email: is_admin checks it.
        let email = self.email_of(user).unwrap_or_default().to_owned();
        let Some(owner) = self.owner.clone() else {
            self.make_owner(&email);
            return Ok(Took::Owner { owner: email });
        };
        if owner == email {
            return Err(format!("{email} is the owner of this riff already"));
        }
        if let Some(take) = &self.take {
            return Err(format!(
                "{} asked for the owner role first; wait for the answer of the owner",
                take.admin
            ));
        }
        self.take = Some(Take {
            admin: email.clone(),
            until: now + wait,
        });
        Ok(Took::Asked {
            owner,
            admin: email,
        })
    }

    /// The owner keeps the owner role that an admin asks for
    /// (01M3N7K41N03P26BEFFNX5617K). `user` is the USER of the caller.
    /// Returns the email of the admin that asked.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// tokens.add_admin("bob@gmail.com").unwrap();
    /// tokens.admit("bob@gmail.com", false, &[], "k2", now).unwrap();
    /// assert!(tokens.deny_owner("ada").is_err(), "no request waits");
    ///
    /// let wait = Duration::from_secs(600);
    /// tokens.take_owner("bob", &[], wait, now).unwrap();
    /// assert!(tokens.deny_owner("bob").is_err(), "only the owner denies");
    /// assert_eq!(tokens.deny_owner("ada").unwrap(), "bob@gmail.com");
    /// assert_eq!(tokens.owner_due(now + wait), None);
    /// assert!(tokens.is_owner("ada"));
    /// ```
    pub fn deny_owner(&mut self, user: &str) -> Result<String, String> {
        if self.owner.is_none() {
            return Err(NO_OWNER.into());
        }
        if !self.is_owner(user) {
            return Err(format!(
                "{user} is not the owner; only the owner denies the owner role"
            ));
        }
        let take = self
            .take
            .take()
            .ok_or("no admin asks for the owner role now")?;
        Ok(take.admin)
    }

    /// The email of the admin whose request waits, or `None`.
    pub fn asks(&self) -> Option<&str> {
        self.take.as_ref().map(|t| t.admin.as_str())
    }

    /// Grants a request whose time ended with no answer of the owner
    /// (01M3N7K41N03P26BEFFNX5617K). The old owner stays an admin.
    /// `None` when no request waits, or its time did not end yet.
    pub fn owner_due(&mut self, now: Instant) -> Option<OwnerChange> {
        if self.take.as_ref().is_none_or(|t| now < t.until) {
            return None;
        }
        let take = self.take.take()?;
        let old = self.owner.clone()?;
        self.make_owner(&take.admin);
        Some(OwnerChange::Granted {
            owner: take.admin,
            old,
        })
    }

    /// True when a request waits and its time ended at `now`.
    pub fn is_due(&self, now: Instant) -> bool {
        self.take.as_ref().is_some_and(|t| t.until <= now)
    }

    /// The owner is gone (01M3N7K46H5BRFJCB46P3JNAFZ). The old owner
    /// stays an admin. The admin of a request that waits is the owner at
    /// once. With no request, the riff has no owner
    /// (01M3N7K48XQ8XSP7R0HD535ZX3). `None` when the riff has no owner.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{OwnerChange, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// assert_eq!(tokens.owner_gone(), Some(OwnerChange::Gone {
    ///     old: "ada@gmail.com".into(),
    ///     owner: None,
    /// }));
    /// assert_eq!(tokens.owner(), None);
    /// assert!(tokens.is_admin("ada", &[]));
    ///
    /// // A riff with no owner gets none at a sign-in, nor from a setting.
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// assert_eq!(tokens.name_owner("ada@gmail.com"), None);
    /// assert!(tokens.owned());
    /// ```
    pub fn owner_gone(&mut self) -> Option<OwnerChange> {
        let old = self.owner.take()?;
        self.members.insert(old.clone());
        self.admins.insert(old.clone());
        let owner = self.take.take().map(|take| {
            self.make_owner(&take.admin);
            take.admin
        });
        self.no_owner = owner.is_none();
        Some(OwnerChange::Gone { old, owner })
    }

    /// Makes `email` the owner, and ends a request that waits. The old
    /// owner, if any, stays an admin and a member.
    fn make_owner(&mut self, email: &str) {
        self.members.remove(email);
        self.admins.remove(email);
        if let Some(old) = self.owner.take() {
            self.members.insert(old.clone());
            self.admins.insert(old);
        }
        self.owner = Some(email.to_owned());
        self.no_owner = false;
        self.take = None;
    }

    /// The ID of this riff (see "Riff ID" in the module docs).
    pub fn riff_id(&self) -> &str {
        &self.riff_id.0
    }

    /// The email of the owner, or `None` before the first sign-in.
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// The owner for `who` (01M3N754NY5JX4P0SN8R4ZYFG9): the USER that
    /// holds the email of the owner, or else the USER that the email
    /// gives. [`RiffOwner::Nobody`] before the first sign-in.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_core::wire::RiffOwner;
    /// use riff_server::token::Tokens;
    ///
    /// let mut tokens = Tokens::default();
    /// assert_eq!(tokens.riff_owner(), RiffOwner::Nobody);
    /// tokens.name_owner("Ada.L@X.io");
    /// let named = RiffOwner::Owner { user: "ada.l".into(), email: "ada.l@x.io".into() };
    /// assert_eq!(tokens.riff_owner(), named);
    /// tokens.admit("ada.l@x.io", false, &[], "k", Instant::now()).unwrap();
    /// assert_eq!(tokens.riff_owner(), named);
    /// ```
    pub fn riff_owner(&self) -> RiffOwner {
        let Some(email) = self.owner.clone() else {
            return RiffOwner::Nobody;
        };
        let user = self
            .user_of_email(&email)
            .map(str::to_owned)
            .or_else(|| user_of(&email).ok())
            .unwrap_or_else(|| email.clone());
        RiffOwner::Owner { user, email }
    }

    /// The emails of the members, sorted.
    pub fn members(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(String::as_str)
    }

    /// True when `user` is an admin: the owner, an admin that the owner
    /// made ([`Tokens::add_admin`]), or an email in `admins` (R210).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k", now).unwrap();
    /// tokens.sign_in("bob@x.io", "k", now).unwrap();
    /// assert!(tokens.is_admin("ada", &[]));
    /// assert!(!tokens.is_admin("bob", &[]));
    /// assert!(tokens.is_admin("bob", &[" Bob@X.io".into()]));
    /// ```
    pub fn is_admin(&self, user: &str, admins: &[String]) -> bool {
        let Some(email) = self.email_of(user) else {
            return false;
        };
        self.owner.as_deref() == Some(email)
            || self.admins.contains(email)
            || admins.iter().any(|a| a.trim().to_lowercase() == email)
    }

    /// True when `user` is the owner.
    pub fn is_owner(&self, user: &str) -> bool {
        self.email_of(user).is_some() && self.email_of(user) == self.owner.as_deref()
    }

    /// The emails of the admins that the owner made, sorted. The admins
    /// of the settings (R210) are not in it.
    pub fn admins(&self) -> impl Iterator<Item = &str> {
        self.admins.iter().map(String::as_str)
    }

    /// Each person once, with the highest role: the owner, the admins
    /// and the members, each sorted (01M3MN157X8N9QKER1AJEPEJVX).
    /// `admins` are the admin emails of the settings (R210).
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k", Instant::now()).unwrap();
    /// tokens.add_admin("bob@gmail.com").unwrap();
    /// tokens.invite("carol@gmail.com").unwrap();
    /// tokens.pass_owner("carol@gmail.com", &[]).unwrap();
    /// let (owner, admins, members) = tokens.roles(&[" Dan@X.io".into()]);
    /// assert_eq!(owner.as_deref(), Some("carol@gmail.com"));
    /// assert_eq!(admins, ["ada@gmail.com", "bob@gmail.com", "dan@x.io"]);
    /// assert!(members.is_empty());
    /// ```
    pub fn roles(&self, admins: &[String]) -> (Option<String>, Vec<String>, Vec<String>) {
        let owner = self.owner.clone();
        let mut all: BTreeSet<String> = self.admins.clone();
        all.extend(admins.iter().map(|a| a.trim().to_lowercase()));
        let not_owner = |email: &String| Some(email) != owner.as_ref();
        let members = self
            .members
            .iter()
            .filter(|m| not_owner(m) && !all.contains(*m))
            .cloned()
            .collect();
        let admins = all.into_iter().filter(not_owner).collect();
        (owner, admins, members)
    }

    /// Makes a person an admin, by verified email
    /// (01M3JY7T109BR860EQBSKEFDHY). The person is also a member, so
    /// stays a member after [`Tokens::remove_admin`]. Returns the email
    /// in lower case. The caller checks that the owner asks.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// assert_eq!(tokens.add_admin(" Bob@gmail.com").unwrap(), "bob@gmail.com");
    /// tokens.admit("bob@gmail.com", false, &[], "k2", now).unwrap();
    /// assert!(tokens.is_admin("bob", &[]));
    ///
    /// assert_eq!(tokens.remove_admin("bob@gmail.com").unwrap(), "bob@gmail.com");
    /// assert!(!tokens.is_admin("bob", &[]));
    /// assert_eq!(tokens.members().collect::<Vec<_>>(), ["bob@gmail.com"]);
    ///
    /// // The owner stays an admin.
    /// assert!(tokens.remove_admin("ada@gmail.com").is_err());
    /// ```
    pub fn add_admin(&mut self, email: &str) -> Result<String, NoSignIn> {
        let email = self.invite(email)?;
        self.admins.insert(email.clone());
        Ok(email)
    }

    /// Makes an admin a member again. The owner stays an admin. Returns
    /// the email in lower case. The caller checks that the owner asks.
    pub fn remove_admin(&mut self, email: &str) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        if self.owner.as_ref() == Some(&email) {
            return Err(format!(
                "{email} is the owner of this riff; the owner stays an admin"
            ));
        }
        if !self.admins.remove(&email) {
            return Err(format!("{email} is not an admin that the owner made"));
        }
        Ok(email)
    }

    /// Passes the owner role to a member or an admin
    /// (01M3JYX8NPZASQY6031R35H39P). `admins` are the admin emails of
    /// the settings (R210). The old owner stays an admin and a member.
    /// Returns the email of the new owner in lower case. The caller
    /// checks that the owner asks.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// tokens.invite("bob@gmail.com").unwrap();
    /// assert_eq!(tokens.pass_owner(" Bob@gmail.com", &[]).unwrap(), "bob@gmail.com");
    /// assert_eq!(tokens.owner(), Some("bob@gmail.com"));
    /// assert_eq!(tokens.admins().collect::<Vec<_>>(), ["ada@gmail.com"]);
    ///
    /// // Only to a member or an admin.
    /// assert!(tokens.pass_owner("carol@gmail.com", &[]).is_err());
    /// ```
    pub fn pass_owner(&mut self, email: &str, admins: &[String]) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        let Some(old) = self.owner.as_ref() else {
            return Err(NO_OWNER.into());
        };
        if *old == email {
            return Err(format!("{email} is the owner of this riff already"));
        }
        let admin = admins.iter().any(|a| a.trim().to_lowercase() == email);
        if !self.members.contains(&email) && !self.admins.contains(&email) && !admin {
            return Err(format!(
                "{email} is not a member of this riff; run riff invite {email} first"
            ));
        }
        self.make_owner(&email);
        Ok(email)
    }

    /// Adds a member by verified email. Returns the email in lower case.
    /// The caller checks that an admin asks.
    pub fn invite(&mut self, email: &str) -> Result<String, NoSignIn> {
        let email = email.trim().to_lowercase();
        user_of(&email).map_err(|e| NoSignIn::Email(e.to_string()))?;
        self.members.insert(email.clone());
        Ok(email)
    }

    /// Removes a member by verified email, and ends each sign-in of that
    /// person (R20). A person of an allowed domain can still sign in.
    /// The owner cannot go. Returns the email in lower case and the
    /// number of sign-ins that ended. The caller checks that an admin
    /// asks.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::{Refused, Tokens};
    ///
    /// let now = Instant::now();
    /// let mut tokens = Tokens::default();
    /// tokens.admit("ada@gmail.com", false, &[], "k1", now).unwrap();
    /// tokens.invite("bob@gmail.com").unwrap();
    /// let bob = tokens.admit("bob@gmail.com", false, &[], "k2", now).unwrap();
    /// assert_eq!(tokens.remove("bob@gmail.com").unwrap(), ("bob@gmail.com".into(), 1));
    /// assert_eq!(tokens.check(&bob.access_token, "k2", now), Err(Refused::Unknown));
    /// assert!(tokens.admit("bob@gmail.com", false, &[], "k2", now).is_err());
    /// assert!(tokens.remove("ada@gmail.com").is_err());
    /// ```
    pub fn remove(&mut self, email: &str) -> Result<(String, usize), String> {
        let email = email.trim().to_lowercase();
        if self.owner.as_ref() == Some(&email) {
            return Err(format!(
                "{email} is the owner of this riff; the owner stays"
            ));
        }
        if self.admins.contains(&email) {
            return Err(format!(
                "{email} is an admin; the owner runs riff admin remove {email} first"
            ));
        }
        self.members.remove(&email);
        let users: Vec<String> = self
            .users
            .iter()
            .filter(|(_, held)| **held == email)
            .map(|(user, _)| user.clone())
            .collect();
        let sign_ins = users.iter().map(|user| self.revoke_user(user)).sum();
        Ok((email, sign_ins))
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
        sign_in.idle_until = now + REFRESH_IDLE;
        let session = refresh.session.clone();
        let gave = refresh.gave;
        if refresh.used.is_some() {
            // The reply of the last use never came back when its pair is
            // unused: a lost reply, not a theft (01M3MX4TG7PNNETZ986DQS10JJ).
            let lost = gave.filter(|g| {
                self.refresh
                    .get(&g.refresh)
                    .is_some_and(|r| r.used.is_none())
            });
            let Some(lost) = lost else {
                self.revoke(id);
                return Err(Refused::Reused);
            };
            self.access.remove(&lost.access);
            self.refresh.remove(&lost.refresh);
        } else {
            refresh.used = Some(now + REUSE_WINDOW);
        }
        let pair = self.issue(id, session, now);
        if let Some(refresh) = self.refresh.get_mut(&hash(token)) {
            refresh.gave = Some(Gave {
                access: hash(&pair.access_token),
                refresh: hash(&pair.refresh_token),
            });
        }
        Ok(pair)
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

    /// The verified email that holds `user` (R209). It is `None` when no
    /// email signed in as `user`.
    ///
    /// ```
    /// use std::time::Instant;
    /// use riff_server::token::Tokens;
    ///
    /// let mut tokens = Tokens::default();
    /// tokens.sign_in("Mike@comotechnologies.io", "k", Instant::now()).unwrap();
    /// assert_eq!(tokens.email_of("mike"), Some("mike@comotechnologies.io"));
    /// assert_eq!(tokens.email_of("brett"), None);
    /// ```
    pub fn email_of(&self, user: &str) -> Option<&str> {
        self.users.get(user).map(String::as_str)
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

    /// The store as JSON, with only the hash of each token (R81). `now`
    /// and `wall` are the same time on the two clocks.
    pub fn to_bytes(&self, now: Instant, wall: SystemTime) -> Vec<u8> {
        let clock = Clock { now, wall };
        let live = |t: Instant| (now < t).then(|| clock.save(t));
        let saved = Saved {
            next_sign_in: self.next_sign_in,
            users: self.users.clone(),
            owner: self.owner.clone(),
            members: self.members.clone(),
            admins: self.admins.clone(),
            riff_id: Some(self.riff_id.0.clone()),
            no_owner: self.no_owner,
            take: self.take.as_ref().map(|t| SavedTake {
                admin: t.admin.clone(),
                until: clock.save(t.until),
            }),
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
                        gave: r.gave.map(|g| SavedGave {
                            access: URL_SAFE_NO_PAD.encode(g.access),
                            refresh: URL_SAFE_NO_PAD.encode(g.refresh),
                        }),
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
            users: saved.users,
            owner: saved.owner,
            members: saved.members,
            admins: saved.admins,
            // A saved form from before the riff ID gets a new one.
            riff_id: saved.riff_id.map(RiffId).unwrap_or_default(),
            no_owner: saved.no_owner,
            // A time that ended while the server was down ends now.
            take: saved.take.map(|t| Take {
                admin: t.admin,
                until: clock.load(t.until).unwrap_or(now),
            }),
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
            let gave = match r.gave {
                Some(g) => Some(Gave {
                    access: unhash(&g.access)?,
                    refresh: unhash(&g.refresh)?,
                }),
                None => None,
            };
            let refresh = Refresh {
                sign_in: r.sign_in,
                session: r.session,
                used,
                gave,
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
                gave: None,
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

/// The saved form. Each time is in milliseconds since the Unix epoch.
#[derive(Serialize, Deserialize)]
struct Saved {
    next_sign_in: u64,
    /// The verified email of each USER (R209).
    users: BTreeMap<String, String>,
    /// The owner and the members (01M3JN3ANE676DT5WQ2NTG47DK). A saved form from
    /// before them has none.
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    members: BTreeSet<String>,
    /// The admins that the owner made (01M3JY7T3645CMQ8CS4T4ABZTP). A
    /// saved form from before them has none.
    #[serde(default)]
    admins: BTreeSet<String>,
    /// The riff ID (01M3JNVBPMZ1K9WX7Q7DP6Y0DH).
    #[serde(default)]
    riff_id: Option<String>,
    /// A riff whose owner was gone, and the request for the owner role
    /// that waits (01M3N7K4GAKJ621V5AWJRQVF3M). A saved form from before
    /// them has neither.
    #[serde(default)]
    no_owner: bool,
    #[serde(default)]
    take: Option<SavedTake>,
    sign_ins: Vec<SavedSignIn>,
    access: Vec<SavedAccess>,
    refresh: Vec<SavedRefresh>,
}

#[derive(Serialize, Deserialize)]
struct SavedTake {
    admin: String,
    until: u64,
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
    /// A saved form from before it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gave: Option<SavedGave>,
}

#[derive(Serialize, Deserialize)]
struct SavedGave {
    access: String,
    refresh: String,
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

    #[test]
    fn a_lost_reply_of_a_session_refresh_keeps_the_session() {
        let (mut tokens, person, now) = signed_in();
        let first = tokens
            .for_session(&person.access_token, "k", "a", now)
            .unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let again = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        let who = tokens.caller(&again.access_token, "k", now).unwrap();
        assert_eq!(who.to_string(), "mike/a");
        assert_eq!(
            tokens.check(&person.access_token, "k", now),
            Ok("mike".to_owned())
        );
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
    fn a_user_belongs_to_one_email() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        let first = tokens.sign_in("o'brien@a.io", "k", now).unwrap();
        assert_eq!(first.user, "o-brien");
        // A second email that gives the same USER does not get it.
        let taken = tokens.sign_in("o-brien@a.io", "k", now);
        assert_eq!(taken, Err(NoSignIn::Taken("o-brien".into())));
        // The email that holds the USER still signs in, in any case.
        let again = tokens.sign_in("O'Brien@A.io", "k", now).unwrap();
        assert_eq!(again.user, "o-brien");
        assert_eq!(tokens.email_of("o-brien"), Some("o'brien@a.io"));
    }

    #[test]
    fn two_domains_do_not_share_a_user() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        tokens.sign_in("alice@a.io", "k", now).unwrap();
        let taken = tokens.sign_in("alice@b.io", "k", now);
        assert_eq!(taken, Err(NoSignIn::Taken("alice".into())));
        assert_eq!(tokens.email_of("alice"), Some("alice@a.io"));
    }

    #[test]
    fn a_revoke_leaves_the_user_of_the_person() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        tokens.sign_in("alice@a.io", "k", now).unwrap();
        assert_eq!(tokens.revoke_user("alice"), 1);
        assert_eq!(tokens.email_of("alice"), Some("alice@a.io"));
        assert!(tokens.sign_in("alice@b.io", "k", now).is_err());
        assert!(tokens.sign_in("alice@a.io", "k", now).is_ok());
    }

    #[test]
    fn a_restart_keeps_the_user_of_each_email() {
        let (tokens, _, now) = signed_in();
        let (mut loaded, now) = restart(&tokens, now, REFRESH_IDLE);
        // Each sign-in ended, and the USER still belongs to its email.
        assert!(loaded.sign_ins.is_empty());
        assert_eq!(loaded.email_of("mike"), Some("mike@comotechnologies.io"));
        let taken = loaded.sign_in("mike@other.io", "k", now);
        assert_eq!(taken, Err(NoSignIn::Taken("mike".into())));
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
    fn the_admins_stay_after_a_restart() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        tokens
            .admit("ada@gmail.com", false, &[], "k1", now)
            .unwrap();
        tokens.add_admin("bob@gmail.com").unwrap();
        tokens
            .admit("bob@gmail.com", false, &[], "k2", now)
            .unwrap();
        let (loaded, _) = restart(&tokens, now, Duration::from_secs(5));
        assert_eq!(loaded.admins().collect::<Vec<_>>(), ["bob@gmail.com"]);
        assert!(loaded.is_admin("bob", &[]));
        assert!(loaded.is_owner("ada"));
        assert!(!loaded.is_owner("bob"));
    }

    #[test]
    fn a_passed_owner_role_stays_after_a_restart_with_the_old_setting() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        tokens.name_owner("ada@gmail.com");
        tokens.invite("bob@gmail.com").unwrap();
        tokens.pass_owner("bob@gmail.com", &[]).unwrap();
        let (mut loaded, _) = restart(&tokens, now, Duration::from_secs(5));
        // The old `--owner` setting names the owner only of a new riff.
        assert_eq!(loaded.name_owner("ada@gmail.com"), Some("bob@gmail.com"));
        assert_eq!(loaded.admins().collect::<Vec<_>>(), ["ada@gmail.com"]);
        assert_eq!(loaded.members().collect::<Vec<_>>(), ["ada@gmail.com"]);
    }

    /// A riff with no owner, and a request that waits, stay after a
    /// restart (01M3N7K4GAKJ621V5AWJRQVF3M).
    #[test]
    fn no_owner_and_a_request_stay_after_a_restart() {
        let now = Instant::now();
        let wait = Duration::from_secs(600);
        let mut tokens = Tokens::default();
        tokens
            .admit("ada@gmail.com", false, &[], "k1", now)
            .unwrap();
        tokens.add_admin("bob@gmail.com").unwrap();
        tokens
            .admit("bob@gmail.com", false, &[], "k2", now)
            .unwrap();
        tokens.take_owner("bob", &[], wait, now).unwrap();

        let (mut loaded, later) = restart(&tokens, now, Duration::from_secs(5));
        assert_eq!(loaded.asks(), Some("bob@gmail.com"));
        assert!(!loaded.is_due(later));
        assert!(loaded.is_due(later + wait));

        // A request whose time ended while the server was down is due at
        // the load.
        let (loaded_late, late) = restart(&tokens, now, wait * 2);
        assert!(loaded_late.is_due(late));

        loaded.deny_owner("ada").unwrap();
        loaded.owner_gone().unwrap();
        let (mut again, _) = restart(&loaded, later, Duration::from_secs(5));
        assert_eq!(again.owner(), None);
        assert!(again.owned());
        assert_eq!(again.name_owner("ada@gmail.com"), None);
        again
            .admit("carol@gmail.com", true, &[], "k3", later)
            .unwrap();
        assert_eq!(again.owner(), None, "a sign-in makes no owner");
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
    fn the_owner_role_passes_only_to_a_member_or_an_admin() {
        let mut tokens = Tokens::default();
        assert!(tokens.pass_owner("bob@gmail.com", &[]).is_err());
        tokens.name_owner("ada@gmail.com");
        let refused = tokens.pass_owner("bob@gmail.com", &[]).unwrap_err();
        assert!(refused.contains("riff invite"), "{refused}");
        assert!(tokens.pass_owner("ada@gmail.com", &[]).is_err());
        // An admin of the settings may take the role.
        let admins = ["Bob@gmail.com".to_owned()];
        assert_eq!(
            tokens.pass_owner("bob@gmail.com", &admins).unwrap(),
            "bob@gmail.com"
        );
        // The role goes back: the admin that the owner made takes it.
        tokens.pass_owner("ada@gmail.com", &[]).unwrap();
        assert_eq!(tokens.owner(), Some("ada@gmail.com"));
        assert_eq!(tokens.admins().collect::<Vec<_>>(), ["bob@gmail.com"]);
    }

    #[test]
    fn an_admin_is_removed_only_after_the_role() {
        let now = Instant::now();
        let mut tokens = Tokens::default();
        tokens
            .admit("ada@gmail.com", false, &[], "k1", now)
            .unwrap();
        tokens.add_admin("bob@gmail.com").unwrap();
        let refused = tokens.remove("bob@gmail.com").unwrap_err();
        assert!(refused.contains("riff admin remove"), "{refused}");
        tokens.remove_admin("bob@gmail.com").unwrap();
        assert!(tokens.remove("bob@gmail.com").is_ok());
        assert!(tokens.remove_admin("bob@gmail.com").is_err());
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
        }
        assert!(text.contains(&URL_SAFE_NO_PAD.encode(hash(&second.access_token))));
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
    fn a_bad_saved_form_does_not_load() {
        let (now, wall) = (Instant::now(), SystemTime::now());
        assert!(Tokens::from_bytes(b"{", now, wall).is_err());
        let bad_hash = br#"{"next_sign_in":1,"users":{},"sign_ins":[],"access":[],
            "refresh":[{"hash":"abc","sign_in":0,"session":null,"used":null}]}"#;
        assert!(Tokens::from_bytes(bad_hash, now, wall).is_err());
        let bad_id = br#"{"next_sign_in":0,"users":{},"sign_ins":[
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
        let second = tokens.refresh(&first.refresh_token, "k", now).unwrap();
        tokens.refresh(&second.refresh_token, "k", now).unwrap();
        tokens.refresh(&first.refresh_token, "k", now).unwrap_err();
        assert_eq!(
            tokens.check(&person.access_token, "k", now),
            Err(Refused::Unknown)
        );
    }
}
