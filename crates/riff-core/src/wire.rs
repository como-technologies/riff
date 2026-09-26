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
//! (R104).
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

use serde::{Deserialize, Serialize};

use crate::name::{SessionUri, ThreadName};
use crate::selector::Selector;

/// `POST /v1/register`: a session says that it exists and where it
/// works. A session registers when it starts and when it moves. It
/// joins the thread of its repository.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Register {
    pub me: SessionUri,
}

/// `POST /v1/who`: lists the known sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhoRequest {}

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub me: SessionUri,
    #[serde(default)]
    pub thread: Option<ThreadName>,
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    /// The URI of the sender when it posted.
    pub from: SessionUri,
    /// The address of the post.
    #[serde(default)]
    pub to: Vec<Selector>,
    pub body: String,
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
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

/// An event on `GET /v1/watch?uri=…`: a message addressed to the
/// session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    pub thread: ThreadName,
    pub seq: u64,
    pub from: SessionUri,
}

/// An event on `GET /v1/tail?thread=…`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tailed {
    pub thread: ThreadName,
    pub message: Message,
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

/// An OAuth error reply, for example `{"error":"invalid_grant"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenError {
    pub error: String,
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
