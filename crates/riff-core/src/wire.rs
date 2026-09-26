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
//! | `who` | [`Who`] | [`WhoReply`] |
//! | `threads` | [`Threads`] | [`ThreadsReply`] |
//! | `join`, `leave` | [`Membership`] | `null` |
//! | `post` | [`Post`] | [`Posted`] |
//! | `tell` | [`Tell`] | [`Posted`] |
//! | `read` | [`Read`] | [`ReadReply`] |
//! | `claim` | [`Claim`] | [`ClaimReply`] |
//! | `release` | [`Claim`] | `null` |
//!
//! Two streams use server-sent events. Each event is one `data:` line
//! that holds JSON:
//!
//! | Stream | Query | Event |
//! |---|---|---|
//! | `GET /v1/watch` | `name=<session name>` | [`Wake`] |
//! | `GET /v1/tail` | `thread=<thread name>` | [`Tailed`] |
//!
//! A session is live while its watch stream is open. Slice 1 has no
//! sign-in: a request names its session in the body.
//!
//! # Example
//!
//! The event that wakes a session after a mention:
//!
//! ```
//! use riff_core::wire::{Wake, WakeReason};
//!
//! let wake = Wake {
//!     thread: "como-technologies/riff".parse()?,
//!     seq: 2,
//!     from: "riff://mike@pangolin/como-technologies/riff#api".parse()?,
//!     reason: WakeReason::Mention,
//! };
//! assert_eq!(
//!     serde_json::to_string(&wake).unwrap(),
//!     r#"{"thread":"como-technologies/riff","seq":2,"from":"riff://mike@pangolin/como-technologies/riff#api","reason":"mention"}"#
//! );
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

use serde::{Deserialize, Serialize};

use crate::name::{SessionName, ThreadName};

/// `POST /v1/register`: a session says that it exists.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Register {
    pub name: SessionName,
}

/// `POST /v1/who`: lists the known sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Who {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhoReply {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: SessionName,
    /// True while the session has an open watch stream.
    pub live: bool,
}

/// `POST /v1/threads`: lists the threads, with the unread count for `name`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Threads {
    pub name: SessionName,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadsReply {
    pub threads: Vec<ThreadInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub thread: ThreadName,
    pub members: Vec<SessionName>,
    pub unread: usize,
}

/// `POST /v1/join` and `POST /v1/leave`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    pub name: SessionName,
    pub thread: ThreadName,
}

/// `POST /v1/post`: adds a message to a thread. The sender joins the thread.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub from: SessionName,
    pub thread: ThreadName,
    pub body: String,
}

/// `POST /v1/tell`: sends a direct message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tell {
    pub from: SessionName,
    pub to: SessionName,
    pub body: String,
}

/// The reply to `post` and `tell`. It tells the sender who woke.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Posted {
    pub thread: ThreadName,
    pub seq: u64,
    /// Each session that the message woke.
    #[serde(default)]
    pub woken: Vec<SessionName>,
    /// Each mention that matched no known session, without the `@`.
    #[serde(default)]
    pub unmatched: Vec<String>,
}

/// `POST /v1/read`: returns the messages that `name` has not read, or
/// all of them when `all` is true. It marks them as read.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Read {
    pub name: SessionName,
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
    pub from: SessionName,
    pub body: String,
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
}

/// `POST /v1/claim` and `POST /v1/release`: a lease on one work item.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub name: SessionName,
    pub thread: ThreadName,
    pub item: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimReply {
    pub granted: bool,
    pub holder: SessionName,
}

/// Why a session wakes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeReason {
    Direct,
    Mention,
}

/// An event on `GET /v1/watch?name=…`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    pub thread: ThreadName,
    pub seq: u64,
    pub from: SessionName,
    pub reason: WakeReason,
}

/// An event on `GET /v1/tail?thread=…`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tailed {
    pub thread: ThreadName,
    pub message: Message,
}
