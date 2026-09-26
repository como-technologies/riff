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
//! [`TokenError`] with status 400.
//!
//! Two streams use server-sent events. Each event is one `data:` line
//! that holds JSON:
//!
//! | Stream | Query | Event |
//! |---|---|---|
//! | `GET /v1/watch` | `uri=<session URI>` | [`Wake`] |
//! | `GET /v1/tail` | `thread=<thread name>` | [`Tailed`] |
//!
//! A session is live while its watch stream is open. There is no
//! sign-in yet: each request carries the session URI of its sender as
//! `me`. The server finds the session by the *who* part of that URI.
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

/// `POST /v1/token`, as `application/x-www-form-urlencoded`: swaps a
/// refresh token for a new pair of tokens. The only grant type is
/// `refresh_token`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub refresh_token: String,
}

/// A new pair of riff tokens. Use `access_token` as a bearer token.
/// Use `refresh_token` once, to get the next pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenReply {
    pub access_token: String,
    /// Always `Bearer`.
    pub token_type: String,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    pub refresh_token: String,
}

/// An OAuth error reply, for example `{"error":"invalid_grant"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenError {
    pub error: String,
}
