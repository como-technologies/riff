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
//! | `riff workers` | Starts, lists and stops the worker sessions of this machine in tmux, with [`terminal`]. Its limit is in [`settings`]. The `riff mcp` of the lead starts workers by itself, with [`rollout`]. [`monitor`] tells the lead about the health of a machine. |
//! | `riff server`, `riff update` | Show the riffs, and update riff on this machine. See [`lifecycle`], and [`auto_update`] for a machine that updates riff by itself. |
//! | `riff login`, `riff logout` | Sign in to the server, or out. See [`login`]. |
//! | `riff pr open`, `riff pr wait`, `riff verify` | The steps of a pull request on GitHub, with `gh`. See [`pr`]. |
//! | `riff usage` | Shows the tokens and the models of an issue, of a wave, or of the sessions of this machine. See [`usage`]. |
//! | `riff audit` | Checks from the log and from `gh` that a wave followed the rules. See [`audit`]. |
//! | `riff cloud` | Makes and runs riff-server instances on Cloud Run, with `gcloud`. See [`cloud`]. |
//!
//! `riff --help` lists the commands under headings (see [`help`]).
//!
//! A session can leave the riff and join it again with the tools
//! `leave` and `join` (see [`leave`]).
//!
//! A granted claim and a new start show the earlier work on an item:
//! its pushed branch and its worktree (see [`dropped`]).
//!
//! The Claude Code plugin is in [`plugin`]. Its start hook runs
//! `riff hook session-start` (see [`hook`]). riff is off in a session
//! until a person turns it on for the repository, with `riff enable`
//! (see [`enable`]).
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
//! only as their session (see [`api`]). The tests and `just dev` give
//! riff a home of its own, with its secrets in files (see [`home`]).
//!
//! ## Messages are advice
//!
//! Each `read` result starts with [`text::DATA_NOTE`] (see [`text::inbox`]). It tells the agent
//! that only a verified message of the lead of its user counts as its
//! user. Each other message is advice (R10).

pub mod activity;
pub mod api;
pub mod audit;
pub mod auto_update;
pub mod binary;
pub mod chat;
pub mod cloud;
pub mod compact;
pub mod device;
pub mod disk;
pub mod dropped;
pub mod enable;
pub mod help;
pub mod home;
pub mod hook;
pub mod host;
pub mod hygiene;
pub mod identity;
pub mod jobserver;
pub mod leave;
pub mod lifecycle;
pub mod limits;
pub mod local;
pub mod login;
pub mod look;
pub mod machine;
pub mod mcp;
pub mod monitor;
pub mod next;
pub mod permissions;
pub mod plugin;
pub mod pr;
pub mod reap;
pub mod relay;
pub mod rollout;
pub mod sccache;
pub mod secrets;
pub mod settings;
pub mod smoke;
pub mod state;
pub mod style;
pub mod temp;
pub mod terminal;
pub mod text;
pub mod tidy;
pub mod top;
pub mod usage;
pub mod view;
pub mod worker;
pub mod worker_lsp;
pub mod worker_mcp;
pub mod workload;
pub mod worktrees;
