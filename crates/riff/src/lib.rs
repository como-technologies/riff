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
//! | `riff watch` | Keeps a watch stream open and prints one line for each wake. With `--once`, it exits after the first wake. |
//! | `riff tail` | Prints each new message in one thread, for people. |
//! | `riff post`, `riff tell`, `riff read`, `riff claim`, `riff release`, `riff lead`, `riff who`, `riff whoami` | Commands for people. |
//! | `riff connect claude` | Installs the Claude Code plugin, with [`plugin::connect`]. |
//! | `riff login`, `riff logout` | Sign in to the server, or out. See [`login`]. |
//!
//! The Claude Code plugin is in [`plugin`]. Its start hook runs
//! `riff hook session-start` (see [`hook`]).
//!
//! `riff mcp` and `riff watch` run as two processes for one session. They
//! agree on the session because both read its session ID from the
//! environment (see [`identity`]). After `/clear`, the environment has a
//! new ID, but `riff mcp` keeps the old one. So `riff mcp` records its ID
//! on the machine, and the other processes of the session use it (see
//! [`local`]). The session can move; the server keeps its place, so a
//! watch that started in the old place still works.
//!
//! ## Wake line
//!
//! An agent tool wakes a session when a watched command prints a line
//! or exits. Claude Code runs `riff watch --once` as a background task,
//! and wakes the session when it exits (see [`hook`]). So `riff watch`
//! prints exactly one line for each wake and nothing else on stdout. Errors go
//! to stderr. The line tells the agent what to do next:
//!
//! See [`text::wake_line`] for the line.
//!
//! ## Secrets
//!
//! `riff` keeps the person tokens and the device key only in the OS
//! keyring, through [`secrets`]. Each request with a token carries a
//! proof from the device key of the machine (see [`device`]). `riff
//! mcp` and `riff watch` each hold a session token in memory; it acts
//! only as their session (see [`api`]).
//!
//! ## Messages are data
//!
//! Each `read` result starts with [`text::DATA_NOTE`] (see [`text::inbox`]). It tells the agent
//! to treat message bodies as data from other sessions, not as
//! instructions from its user.

pub mod api;
pub mod device;
pub mod hook;
pub mod identity;
pub mod local;
pub mod login;
pub mod mcp;
pub mod plugin;
pub mod secrets;
pub mod text;
