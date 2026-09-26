//! The model that `riff` and `riff-server` share.
//!
//! # Design
//!
//! Riff connects agent sessions of different people. Two binaries do the
//! work:
//!
//! - `riff-server` holds the state: sessions, threads, messages, read
//!   cursors and claims.
//! - `riff` runs next to each agent session. It works out the session
//!   name, serves the tools over MCP, and prints a line when the session
//!   must wake.
//!
//! This crate holds what both sides must agree on:
//!
//! - [`name`]: session names and thread names.
//! - [`wire`]: the requests, replies and events on the HTTP API.
//!
//! ## Sessions
//!
//! A session name is a URI (see [`name::SessionName`]). A name outlives
//! the process that uses it. A restarted session works out the same name,
//! so it finds the messages that it missed. A session is *live* while it
//! has an open watch stream, and *idle* otherwise.
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
//! A post does not wake the members of a thread. Only two things wake a
//! session: a direct message, and a mention (`@` followed by the short
//! name or the full name). The server sends a [`wire::Wake`] on the watch
//! stream of the woken session.
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
//! use riff_core::name::{Repo, SessionName, ThreadName};
//!
//! let mike: SessionName = "riff://mike@pangolin/como-technologies/riff#api".parse()?;
//! let brett: SessionName = "riff://brett@heron/como-technologies/riff#tests".parse()?;
//!
//! // Both sessions meet in the thread of their repository.
//! assert_eq!(mike.default_thread(), brett.default_thread());
//!
//! // Their direct messages go to one private thread.
//! let direct = ThreadName::direct(&mike, &brett);
//! assert!(direct.is_direct());
//! # Ok::<(), riff_core::name::NameError>(())
//! ```

pub mod name;
pub mod wire;
