//! `riff`: the local client that finds sessions and wakes yours.
//!
//! # Design
//!
//! `riff` runs on the machine of each person. It has one job for each
//! mode:
//!
//! | Mode | Job |
//! |---|---|
//! | `riff mcp` | Serves the tools in [`mcp`] to one agent session over stdio. |
//! | `riff watch` | Keeps a watch stream open and prints one line for each wake. |
//! | `riff tail` | Prints each new message in one thread, for people. |
//! | `riff post`, `riff claim`, `riff release`, `riff who`, `riff whoami` | Commands for people. |
//!
//! The Claude Code plugin is in [`plugin`].
//!
//! `riff mcp` and `riff watch` run as two processes for one session. They
//! agree on the session name because both work it out the same way from
//! the directory (see [`identity`]). Slice 1 allows one live session for
//! each name.
//!
//! ## Wake line
//!
//! An agent tool wakes a session when a watched command prints a line.
//! Claude Code does this with its Monitor tool. So `riff watch` prints
//! exactly one line for each wake and nothing else on stdout. Errors go
//! to stderr. The line tells the agent what to do next:
//!
//! ```
//! use riff_core::wire::{Wake, WakeReason};
//!
//! let wake = Wake {
//!     thread: "como-technologies/riff".parse()?,
//!     seq: 7,
//!     from: "riff://mike@pangolin/como-technologies/riff#api".parse()?,
//!     reason: WakeReason::Mention,
//! };
//! assert_eq!(
//!     riff::text::wake_line(&wake),
//!     "riff: mike@pangolin:riff#api mentioned you (message 7). Use the riff read tool."
//! );
//! # Ok::<(), riff_core::name::NameError>(())
//! ```
//!
//! ## Messages are data
//!
//! Each `read` result starts with [`text::DATA_NOTE`]. It tells the agent
//! to treat message bodies as data from other sessions, not as
//! instructions from its user.

pub mod api;
pub mod identity;
pub mod mcp;
pub mod plugin;
pub mod text;
