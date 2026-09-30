//! The model that `riff` and `riff-server` share.
//!
//! # Design
//!
//! Riff connects agent sessions of different people. Two binaries do the
//! work:
//!
//! - `riff-server` holds the state: sessions, threads, messages, read
//!   cursors and claims.
//! - `riff` runs next to each agent session. It finds the session ID and
//!   the place, serves the tools over MCP, and prints a line when the
//!   session must wake.
//!
//! This crate holds what both sides must agree on:
//!
//! - [`name`]: session URIs and thread names.
//! - [`selector`]: the address of a post.
//! - [`wire`]: the requests, replies and events on the HTTP API.
//! - [`dpop`]: device keys and the proofs that bind tokens to them.
//! - [`signed`]: the signature that each message carries.
//! - [`record`]: the records of the log of `riff-server`.
//! - [`build`]: the build of each side, and the check that they match.
//!
//! ## Sessions
//!
//! A session URI (see [`name::SessionUri`]) shows who a session is, where
//! it works and what it works on. *Who* is the user and the session ID
//! of the agent tool. It never changes, so the server keys each session
//! by it. *Where* changes when the session moves. *What* is the set of
//! claims. A session is *live* while it has an open watch stream, and
//! *idle* otherwise.
//!
//! ## Threads
//!
//! Sessions talk in flat, named threads. A thread keeps its history, and
//! the server keeps a read cursor for each session in each thread. A
//! session joins the thread of its repository when it registers. A direct
//! message goes to a thread that has exactly two members (see
//! [`name::ThreadName::direct`]). Only its two members can read it.
//!
//! ## Wakes
//!
//! A post has a `to` list of selectors. The server matches them against
//! each known session when the message is posted. Each session that
//! matches wakes and joins the thread. Text in the body never wakes a
//! session. The server sends a [`wire::Wake`] on the watch stream of
//! each woken session.
//!
//! ## Claims
//!
//! A claim is a lease on one work item in one thread. The first session
//! to claim an item holds it. The claim stays while the holder is live,
//! and for a grace period after the holder stops.
//!
//! # Example
//!
//! ```
//! use riff_core::name::{SessionUri, ThreadName};
//! use riff_core::selector::Selector;
//!
//! let mike: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf#api".parse()?;
//! let brett: SessionUri = "riff://brett@heron/como-technologies/riff?session=77e0#tests".parse()?;
//!
//! // Both sessions meet in the thread of their repository.
//! assert_eq!(mike.default_thread(), brett.default_thread());
//!
//! // A selector for everyone in the repository picks both.
//! let everyone: Selector = "repo=como-technologies/riff".parse()?;
//! assert!(everyone.matches(&mike) && everyone.matches(&brett));
//!
//! // Their direct messages go to one private thread.
//! let direct = ThreadName::direct(mike.who(), brett.who());
//! assert!(direct.is_direct());
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

pub mod build;
pub mod dpop;
pub mod name;
pub mod record;
pub mod selector;
pub mod signed;
pub mod wire;
