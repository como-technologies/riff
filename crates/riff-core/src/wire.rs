//! Requests, replies and events between `riff` and `riff-server`.
//!
//! # Protocol
//!
//! Each call is `POST /v1/<op>` with a JSON body. A reply is JSON with
//! status 200. An error is plain text with status 400 or 404.
//!
//! | Op | Request | Reply |
//! |---|---|---|
//! | `register` | [`Register`] | `null` |
//! | `who` | [`WhoRequest`] | [`WhoReply`] |
//! | `threads` | [`Threads`] | [`ThreadsReply`] |
//! | `join`, `leave` | [`Membership`] | `null` |
//! | `post` | [`Post`] | [`Posted`] |
//! | `read` | [`Read`] | [`ReadReply`] |
//! | `claim` | [`Claim`] | [`ClaimReply`] |
//! | `release` | [`Claim`] | `null` |
//! | `lead` | [`Lead`] | [`LeadReply`] |
//! | `riff` | [`Riff`] | [`RiffReply`] |
//! | `status` | [`SetStatus`] | `null` |
//! | `alive` | [`Alive`] | `null` |
//! | `end` | [`End`] | `null` |
//! | `start` | [`Start`] | [`Started`] |
//!
//! `POST /v1/token` is an OAuth 2.1 token endpoint. Its request is a
//! form, [`TokenRequest`]. Its reply is [`TokenReply`], or
//! [`TokenError`] with status 400. `GET /v1/sign-in` gives
//! [`SignInConfig`], or status 404 when the server has no sign-in
//! provider.
//!
//! `POST /v1/revoke` ends each sign-in of one person (R20). It needs
//! an access token and a DPoP proof. Its request is [`Revoke`] and
//! its reply is [`Revoked`]. A missing or bad token gets status 401. A
//! person who is not an admin and names another user gets status 403.
//!
//! Three calls change or show who may join the riff
//! (01M3JN3AHMK532XMRDASD4XD5D). Each needs an access token and a DPoP
//! proof, like `revoke`:
//!
//! | Path | Request | Reply | Who |
//! |---|---|---|---|
//! | `POST /v1/invite` | [`Invite`] | [`Invited`] | an admin |
//! | `POST /v1/remove` | [`Remove`] | [`Removed`] | an admin |
//! | `POST /v1/members` | [`Members`] | [`MembersReply`] | each person |
//!
//! A person who is not an admin gets status 403 from `invite` and
//! `remove`. `remove` of the owner gets status 400.
//!
//! Two metadata documents follow the MCP authorization spec (R22):
//!
//! | Path | Reply |
//! |---|---|
//! | `GET /.well-known/oauth-protected-resource` | [`ResourceMetadata`] (RFC 9728) |
//! | `GET /.well-known/oauth-authorization-server` | [`ServerMetadata`] (RFC 8414) |
//!
//! Two streams use server-sent events. Each event is one `data:` line
//! that holds JSON:
//!
//! | Stream | Query | Event |
//! |---|---|---|
//! | `GET /v1/watch` | `uri=<session URI>` | [`Wake`] |
//! | `GET /v1/tail` | `thread=<thread name>` | [`Tailed`] |
//!
//! A session is live while its watch stream is open. Each request
//! carries the session URI of its sender as `me`. The server finds the
//! session by the *who* part of that URI. When the server needs
//! sign-in, the *who* part must match the token, or the reply is 403
//! (R104). A session keeps the user of its first call: a known session
//! ID with another user gets 409 (R159).
//! Only `register` sets the place of a known session; each other call
//! uses the URI only to make a session that the server does not know.
//!
//! # Example
//!
//! A post to the holder of a claim:
//!
//! ```
//! use riff_core::wire::Post;
//!
//! let post: Post = serde_json::from_str(r#"{
//!     "me": "riff://mike@pangolin/como-technologies/riff?session=a6cf",
//!     "thread": "como-technologies/riff",
//!     "to": [{ "claim": "issue-6" }],
//!     "body": "The API is ready."
//! }"#).unwrap();
//! assert_eq!(post.to[0].claim.as_deref(), Some("issue-6"));
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dpop::Key;
use crate::name::{SessionUri, ThreadName};
use crate::selector::Selector;
use crate::signed::Content;

/// `POST /v1/register`: a session says that it exists and where it
/// works. A session registers when it starts and when it moves. It
/// joins the thread of its repository.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Register {
    pub me: SessionUri,
}

/// `riff mcp` sends a keep-alive this often, also while no turn runs
/// (R204). A session with no sign of life for 3 minutes is gone.
pub const ALIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// `POST /v1/alive`: a keep-alive. It shows that the session still runs,
/// but it is not a call: the idle time in `who` stays (R204).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alive {
    pub me: SessionUri,
}

/// `POST /v1/end`: the session ended (R205). It leaves `who`, and its
/// claims are free at once.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct End {
    pub me: SessionUri,
}

/// `POST /v1/who`: lists the known sessions. The call counts as a call
/// of `me`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhoRequest {
    pub me: SessionUri,
    /// True lists gone sessions too.
    #[serde(default)]
    pub all: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhoReply {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    /// The URI now: the place and the claims are current.
    pub uri: SessionUri,
    /// True while the session has an open watch stream.
    pub live: bool,
    /// The seconds since the last call of the session. 0 while it is
    /// live.
    #[serde(default)]
    pub idle_secs: u64,
    /// The last status that the session set, if it set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusInfo>,
}

/// The most characters in the step or the reason of a [`Status`].
pub const STATUS_CHARS: usize = 200;

/// What a session does now: its current step, and a reason when it is
/// blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub step: String,
    /// Why the session cannot go on. `None` when it is not blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

impl Status {
    /// Refuses a status that `who` cannot show on one line: an empty
    /// step, a line break, or more than [`STATUS_CHARS`] characters in
    /// the step or the reason.
    ///
    /// ```
    /// use riff_core::wire::Status;
    ///
    /// let status = |step: &str, blocked: Option<&str>| Status {
    ///     step: step.into(),
    ///     blocked: blocked.map(Into::into),
    /// };
    /// assert!(status("write the tests", None).check().is_ok());
    /// assert!(status("merge", Some("waits for a review")).check().is_ok());
    /// assert!(status(" ", None).check().is_err());
    /// assert!(status("merge", Some("")).check().is_err());
    /// assert!(status("two\nlines", None).check().is_err());
    /// assert!(status(&"x".repeat(201), None).check().is_err());
    /// ```
    pub fn check(&self) -> Result<(), String> {
        let parts = [
            ("step", Some(&self.step)),
            ("reason", self.blocked.as_ref()),
        ];
        for (what, text) in parts {
            let Some(text) = text else { continue };
            if text.trim().is_empty() {
                return Err(format!("the {what} of a status is empty"));
            }
            if text.chars().any(char::is_control) {
                return Err(format!("the {what} of a status must be one line"));
            }
            if text.chars().count() > STATUS_CHARS {
                return Err(format!(
                    "the {what} of a status has more than {STATUS_CHARS} characters"
                ));
            }
        }
        Ok(())
    }
}

/// A [`Status`] in the reply to `who`, with its age.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusInfo {
    #[serde(flatten)]
    pub status: Status,
    /// The seconds since the session set the status.
    pub age_secs: u64,
}

/// `POST /v1/status`: sets the status of `me`. It replaces the old
/// status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetStatus {
    pub me: SessionUri,
    pub status: Status,
}

/// `POST /v1/threads`: lists the threads of `me`, with unread counts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Threads {
    pub me: SessionUri,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadsReply {
    pub threads: Vec<ThreadInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub thread: ThreadName,
    pub members: Vec<SessionUri>,
    pub unread: usize,
}

/// `POST /v1/join` and `POST /v1/leave`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    pub me: SessionUri,
    pub thread: ThreadName,
}

/// `POST /v1/post`: adds a message to a thread and wakes each session
/// that `to` selects. The sender and each woken session join the
/// thread.
///
/// With no thread, the post is a direct message: `to` must be one
/// selector with a `session`, and the thread is the direct thread of
/// the two sessions.
///
/// A signed-in sender signs the post with [`Post::sign`] (R195).
///
/// ```
/// use riff_core::dpop::Key;
/// use riff_core::wire::Post;
///
/// let me = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
/// let mut post = Post::new(&me, Some("design".parse()?), vec![], "look");
/// let key = Key::generate();
/// post.sign(&key, 1_000);
/// let content = post.content().unwrap();
/// assert_eq!(content.verify(post.sig.as_ref().unwrap()).unwrap(), key.thumbprint());
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub me: SessionUri,
    #[serde(default)]
    pub thread: Option<ThreadName>,
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
    /// The signed time, in milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    /// The signature of the sender (see [`crate::signed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

impl Post {
    /// A post of kind [`Kind::Message`], with no signature.
    pub fn new(me: &SessionUri, thread: Option<ThreadName>, to: Vec<Selector>, body: &str) -> Post {
        Post {
            me: me.clone(),
            thread,
            to,
            body: body.to_owned(),
            kind: Kind::Message,
            at_ms: None,
            sig: None,
        }
    }

    /// Signs the post with the device key `key`, at the time `at_ms`
    /// (R195, R196).
    pub fn sign(&mut self, key: &Key, at_ms: u64) {
        self.at_ms = Some(at_ms);
        self.sig = self.content().map(|content| content.sign(key));
    }

    /// What the signature covers. `None` when the post has no signed
    /// time.
    pub fn content(&self) -> Option<Content<'_>> {
        Some(Content {
            from: self.me.who(),
            lead: self.me.lead(),
            thread: self.thread.as_ref(),
            to: &self.to,
            body: &self.body,
            kind: self.kind,
            at_ms: self.at_ms?,
        })
    }
}

/// The kind of a post.
///
/// ```
/// use riff_core::wire::Kind;
///
/// assert_eq!(serde_json::to_string(&Kind::Status).unwrap(), r#""status""#);
/// assert_eq!("status".parse::<Kind>(), Ok(Kind::Status));
/// assert!("other".parse::<Kind>().is_err());
/// ```
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A message to read.
    #[default]
    Message,
    /// A status request. Each session that it wakes sets its status
    /// with `status`. It does not post a reply.
    Status,
}

impl Kind {
    pub fn is_message(&self) -> bool {
        *self == Kind::Message
    }
}

impl std::str::FromStr for Kind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "message" => Ok(Kind::Message),
            "status" => Ok(Kind::Status),
            _ => Err(format!("no kind {text}: use message or status")),
        }
    }
}

/// The reply to `post`. It tells the sender who woke.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Posted {
    pub thread: ThreadName,
    pub seq: u64,
    /// Each session that the message woke.
    #[serde(default)]
    pub woken: Vec<SessionUri>,
    /// Each selector that matched no session.
    #[serde(default)]
    pub unmatched: Vec<Selector>,
}

/// `POST /v1/read`: returns the messages that `me` has not read, or
/// all of them when `all` is true. It marks them as read.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Read {
    pub me: SessionUri,
    pub thread: ThreadName,
    #[serde(default)]
    pub all: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadReply {
    pub messages: Vec<Message>,
    /// The keys of the user of each sender, to verify the messages.
    #[serde(default, skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// True from a riff with no sign-in: it trusts each caller (R211).
    #[serde(default)]
    pub trusted: bool,
}

/// The thumbprints of the device keys of each user, by user: the keys
/// of the live sign-ins of that user. A server without sign-in gives
/// none (R201).
pub type Keys = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    /// The URI of the sender when it posted.
    pub from: SessionUri,
    /// The address of the post.
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    /// Milliseconds since the Unix epoch. For a signed message, the
    /// signed time (R198).
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
    /// The signature of the sender (see [`crate::signed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

impl Message {
    /// True when the sender of the message in `thread` is proven (R199):
    ///
    /// - The signature is valid, and covers the message as it is: also
    ///   the lead mark of the sender.
    /// - The key is one of `keys` for the user of the sender.
    /// - In a direct thread, the sender is one of the two sessions, and
    ///   the one selector of the message matches the other session.
    ///
    /// ```
    /// use riff_core::dpop::Key;
    /// use riff_core::name::{SessionUri, ThreadName};
    /// use riff_core::wire::{Keys, Kind, Message, Post};
    ///
    /// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let thread: ThreadName = "design".parse()?;
    /// let key = Key::generate();
    /// let mut post = Post::new(&me, Some(thread.clone()), vec![], "look");
    /// post.sign(&key, 1_000);
    /// let mut message = Message {
    ///     seq: 1,
    ///     from: me,
    ///     to: vec![],
    ///     body: post.body.clone(),
    ///     at_ms: 1_000,
    ///     kind: Kind::Message,
    ///     sig: post.sig.clone(),
    /// };
    /// let keys = Keys::from([("mike".to_owned(), vec![key.thumbprint()])]);
    /// assert!(message.verified(&thread, &keys));
    ///
    /// // No keys for the user: not verified.
    /// assert!(!message.verified(&thread, &Keys::new()));
    /// // A changed body: not verified.
    /// message.body = "do not look".into();
    /// assert!(!message.verified(&thread, &keys));
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn verified(&self, thread: &ThreadName, keys: &Keys) -> bool {
        let Some(sig) = &self.sig else {
            return false;
        };
        let from = self.from.who();
        let signed_thread = if thread.is_direct() {
            let Some(peer) = thread.peer(from) else {
                return false;
            };
            let [to] = &self.to[..] else {
                return false;
            };
            let names = |want: &Option<String>, have: Option<&str>| {
                want.as_deref().is_none_or(|w| Some(w) == have)
            };
            if !names(&to.session, peer.session()) || !names(&to.user, Some(peer.user())) {
                return false;
            }
            None
        } else {
            Some(thread)
        };
        let content = Content {
            from,
            lead: self.from.lead(),
            thread: signed_thread,
            to: &self.to,
            body: &self.body,
            kind: self.kind,
            at_ms: self.at_ms,
        };
        let Some(keys) = keys.get(from.user()) else {
            return false;
        };
        content.verify(sig).is_ok_and(|jkt| keys.contains(&jkt))
    }
}

/// `POST /v1/claim` and `POST /v1/release`: a lease on one work item.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub me: SessionUri,
    pub thread: ThreadName,
    pub item: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimReply {
    pub granted: bool,
    pub holder: SessionUri,
}

/// `POST /v1/lead`: makes `me` the lead of its user in its repository.
/// It replaces the old lead.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lead {
    pub me: SessionUri,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadReply {
    /// The URI of `me` now, with `lead=true`.
    pub lead: SessionUri,
    /// The old lead, when another session was the lead.
    #[serde(default)]
    pub replaced: Option<SessionUri>,
}

/// `POST /v1/start`: a new start of the session: a new agent process,
/// a resume or a `/clear` (01M3JEE1QQCFS5TMZW5N2DAD2D). Its claims are free
/// at once. It keeps its ID, its threads and its lead.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Start {
    pub me: SessionUri,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Started {
    /// Each claim that the start freed.
    #[serde(default)]
    pub freed: Vec<Freed>,
}

/// A claim that a new start freed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freed {
    pub thread: ThreadName,
    pub item: String,
}

/// `POST /v1/riff`: reads the state of the riff. With a `state`, it
/// sets it. Only a person (a `me` with no session ID) or a lead can set
/// it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Riff {
    pub me: SessionUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<RiffState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiffReply {
    /// The state now.
    pub state: RiffState,
    /// True when the call changed the state.
    #[serde(default)]
    pub changed: bool,
}

/// The state of a riff. A new riff is paused.
///
/// ```
/// use riff_core::wire::RiffState;
///
/// assert_eq!(RiffState::default(), RiffState::Paused);
/// assert_eq!(serde_json::to_string(&RiffState::Running).unwrap(), r#""running""#);
/// assert_eq!(RiffState::Running.to_string(), "running");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiffState {
    /// The sessions stop at their next step and wait. Nobody claims.
    #[default]
    Paused,
    /// The sessions pick and do work.
    Running,
}

impl std::fmt::Display for RiffState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RiffState::Paused => "paused",
            RiffState::Running => "running",
        })
    }
}

/// An event on `GET /v1/watch?uri=…`: a message addressed to the
/// session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    pub thread: ThreadName,
    pub seq: u64,
    pub from: SessionUri,
    #[serde(default, skip_serializing_if = "Kind::is_message")]
    pub kind: Kind,
}

/// An event on `GET /v1/tail?thread=…`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tailed {
    pub thread: ThreadName,
    pub message: Message,
    /// The keys of the user of the sender, to verify the message.
    #[serde(default, skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// True from a riff with no sign-in (R211). See
    /// [`ReadReply::trusted`].
    #[serde(default)]
    pub trusted: bool,
}

/// The grant type that swaps an ID token of the sign-in provider for
/// a first pair of riff tokens (RFC 8693 token exchange).
pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

/// The subject token type of a [`TOKEN_EXCHANGE`] request.
pub const ID_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id_token";

/// The subject token type of a [`TOKEN_EXCHANGE`] request that swaps a
/// person access token for a session pair (R19).
pub const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

/// `POST /v1/token`, as `application/x-www-form-urlencoded`.
///
/// | `grant_type` | Fields |
/// |---|---|
/// | `refresh_token` | `refresh_token` |
/// | [`TOKEN_EXCHANGE`] | `subject_token` (an ID token), `subject_token_type` = [`ID_TOKEN_TYPE`] |
/// | [`TOKEN_EXCHANGE`] | `subject_token` (a person access token), `subject_token_type` = [`ACCESS_TOKEN_TYPE`], `session` |
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_token_type: Option<String>,
    /// The session ID of a session pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The server that the token is for (RFC 8707). When it is set, it
    /// must be the public URL of the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

/// A new pair of riff tokens. Send `access_token` with the `DPoP`
/// scheme and a proof from the device key. Use `refresh_token` once, to
/// get the next pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenReply {
    pub access_token: String,
    /// Always `DPoP`.
    pub token_type: String,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    pub refresh_token: String,
    /// The user part of the session URI, from the sign-in (R36).
    pub user: String,
}

/// `GET /v1/sign-in`: the OpenID Connect provider that `riff login`
/// signs in with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInConfig {
    /// The issuer. Its discovery document is at
    /// `<issuer>/.well-known/openid-configuration`.
    pub issuer: String,
    pub client_id: String,
    /// Google asks for the secret of a desktop client too. It is not a
    /// secret: it ships to each person who signs in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

/// The fields that riff uses from the discovery document of an OpenID
/// Connect provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

impl Discovery {
    /// The URL of the discovery document of `issuer`.
    ///
    /// ```
    /// use riff_core::wire::Discovery;
    ///
    /// assert_eq!(
    ///     Discovery::url("https://accounts.google.com/"),
    ///     "https://accounts.google.com/.well-known/openid-configuration"
    /// );
    /// ```
    pub fn url(issuer: &str) -> String {
        format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        )
    }
}

/// `POST /v1/revoke`: ends each sign-in of a person.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revoke {
    /// The person. Leave it out for the caller. Only an admin names
    /// another person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// The reply to [`Revoke`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revoked {
    pub user: String,
    /// The number of sign-ins that ended.
    pub sign_ins: usize,
}

/// `POST /v1/invite`: adds a member, by verified email.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub email: String,
}

/// The reply to [`Invite`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invited {
    /// The email of the member, in lower case.
    pub email: String,
}

/// `POST /v1/remove`: removes a member and ends each sign-in of that
/// person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remove {
    pub email: String,
}

/// The reply to [`Remove`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Removed {
    /// The email, in lower case.
    pub email: String,
    /// The number of sign-ins that ended.
    pub sign_ins: usize,
}

/// `POST /v1/members`: shows who may join the riff.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Members {}

/// The reply to [`Members`].
///
/// ```
/// use riff_core::wire::MembersReply;
///
/// let reply: MembersReply = serde_json::from_str(
///     r#"{"owner":"ada@gmail.com","admins":[],"members":["bob@gmail.com"],"allowed_domains":["x.io"]}"#,
/// ).unwrap();
/// assert_eq!(reply.owner.as_deref(), Some("ada@gmail.com"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersReply {
    /// The email of the owner. `None` before the first sign-in.
    pub owner: Option<String>,
    /// The admin emails of the settings (R210).
    pub admins: Vec<String>,
    /// The email of each member, sorted.
    pub members: Vec<String>,
    /// The allowed domains (R15).
    pub allowed_domains: Vec<String>,
}

/// An OAuth error reply, for example `{"error":"invalid_grant"}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenError {
    pub error: String,
    /// What a person must read, for a refusal that only they can fix,
    /// for example a USER that another account holds (R209).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

/// The protected resource metadata of `riff-server` (RFC 9728).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceMetadata {
    /// The public URL of the server. Tokens are only for it.
    pub resource: String,
    /// The issuer of each token. It is the server itself.
    pub authorization_servers: Vec<String>,
    /// Always `["header"]`: a token goes only in the Authorization header.
    pub bearer_methods_supported: Vec<String>,
    /// Always `["ES256"]` (RFC 9449).
    pub dpop_signing_alg_values_supported: Vec<String>,
    /// Always true: each token is bound to a device key (R18).
    pub dpop_bound_access_tokens_required: bool,
}

/// The authorization server metadata of `riff-server` (RFC 8414).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMetadata {
    pub issuer: String,
    pub token_endpoint: String,
    pub grant_types_supported: Vec<String>,
    /// Empty: the server has no authorization endpoint (R83).
    pub response_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
    /// Always `["none"]`: `riff` is a public client.
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// Always `["ES256"]` (RFC 9449).
    pub dpop_signing_alg_values_supported: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Who;

    const MIKE: &str = "riff://mike@pangolin/como-technologies/riff?session=a6cf";
    const BRETT: &str = "riff://brett@heron/como-technologies/riff?session=77e0";

    fn uri(text: &str) -> SessionUri {
        text.parse().unwrap()
    }

    /// The message that the server stores for a signed post.
    fn stored(post: &Post, from: SessionUri) -> Message {
        Message {
            seq: 1,
            from,
            to: post.to.clone(),
            body: post.body.clone(),
            at_ms: post.at_ms.unwrap(),
            kind: post.kind,
            sig: post.sig.clone(),
        }
    }

    fn keys(user: &str, key: &Key) -> Keys {
        Keys::from([(user.to_owned(), vec![key.thumbprint()])])
    }

    #[test]
    fn a_direct_message_verifies_only_in_its_own_thread() {
        let key = Key::generate();
        let (mike, brett) = (uri(MIKE), uri(BRETT));
        let to = vec![Selector::session("77e0")];
        let mut post = Post::new(&mike, None, to, "hi");
        post.sign(&key, 5);
        let message = stored(&post, mike.clone());
        let keys = keys("mike", &key);

        let theirs = ThreadName::direct(mike.who(), brett.who());
        assert!(message.verified(&theirs, &keys));

        // The same message in the direct thread of mike and another session.
        let other = Who::new("ada", Some("c3")).unwrap();
        assert!(!message.verified(&ThreadName::direct(mike.who(), &other), &keys));
        // In a direct thread without mike.
        assert!(!message.verified(&ThreadName::direct(brett.who(), &other), &keys));
        // In a named thread.
        assert!(!message.verified(&"design".parse().unwrap(), &keys));
    }

    #[test]
    fn a_direct_message_to_the_lead_must_reach_a_session_of_that_user() {
        let key = Key::generate();
        let mike = uri(MIKE);
        let lead = Who::new("mike", Some("1ead")).unwrap();
        let to = vec![Selector::lead("mike", "como-technologies/riff")];
        let mut post = Post::new(&mike, None, to, "may I?");
        post.sign(&key, 5);
        let message = stored(&post, mike.clone());
        let keys = keys("mike", &key);
        assert!(message.verified(&ThreadName::direct(mike.who(), &lead), &keys));
        let brett = Who::new("brett", Some("77e0")).unwrap();
        assert!(!message.verified(&ThreadName::direct(mike.who(), &brett), &keys));
    }

    #[test]
    fn a_false_sender_is_not_verified() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        let keys = keys("mike", &key);
        assert!(stored(&post, uri(MIKE)).verified(&thread, &keys));

        // The stored sender names the lead: another session of mike.
        let lead = uri("riff://mike@pangolin/como-technologies/riff?session=1ead&lead=true");
        assert!(!stored(&post, lead).verified(&thread, &keys));
        // The key of mike does not count for brett.
        assert!(!stored(&post, uri(BRETT)).verified(&thread, &keys));
    }

    #[test]
    fn the_place_and_the_claims_are_not_signed() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "go");
        post.sign(&key, 5);
        let now = "riff://mike@pangolin/other/repo?session=a6cf&claim=issue-6#api";
        assert!(stored(&post, uri(now)).verified(&thread, &keys("mike", &key)));
    }

    #[test]
    fn the_lead_mark_is_signed() {
        let key = Key::generate();
        let keys = keys("mike", &key);
        let thread: ThreadName = "design".parse().unwrap();
        let lead = uri(&format!("{MIKE}&lead=true"));

        // A lead mark added to the sender after the post.
        let mut post = Post::new(&uri(MIKE), Some(thread.clone()), vec![], "merge now");
        post.sign(&key, 5);
        assert!(stored(&post, uri(MIKE)).verified(&thread, &keys));
        assert!(!stored(&post, lead.clone()).verified(&thread, &keys));

        // A post of the lead: its lead mark is signed.
        let mut post = Post::new(&lead, Some(thread.clone()), vec![], "merge now");
        post.sign(&key, 5);
        assert!(stored(&post, lead).verified(&thread, &keys));
        assert!(!stored(&post, uri(MIKE)).verified(&thread, &keys));
    }

    #[test]
    fn a_message_without_a_signature_is_not_verified() {
        let key = Key::generate();
        let thread: ThreadName = "design".parse().unwrap();
        let message = Message {
            seq: 1,
            from: uri(MIKE),
            to: vec![],
            body: "go".into(),
            at_ms: 5,
            kind: Kind::Message,
            sig: None,
        };
        assert!(!message.verified(&thread, &keys("mike", &key)));
    }
}
