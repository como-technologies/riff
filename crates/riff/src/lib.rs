//! `riff`: the local client that finds sessions and wakes yours.
//!
//! # Design
//!
//! `riff` runs on the machine of each person. It has one job for each
//! mode:
//!
//! | Mode | Job |
//! |---|---|
//! | `riff tail` | Prints each new message in one thread, for people. |
//! | `riff post`, `riff tell`, `riff read`, `riff claim`, `riff release`, `riff lead`, `riff who`, `riff whoami` | Commands for people. |
//! | `riff relay mcp`, `riff relay watch` | The tools and the wakes of a session in a container. See [`tools`], [`session_relay`]. |
//! | `riff server`, `riff update` | Show the riffs, and update riff on this machine. See [`lifecycle`], [`release`] for the static binaries of a release, and [`auto_update`] for a machine that updates riff by itself. |
//! | `riff login`, `riff logout` | Sign in to the server, or out. See [`login`]. |
//! | `riff release cut`, `publish`, `status`, `approve` | The release cycle: the release pull request, the tag, the GitHub release and the approval of the deploy. See [`cycle`]. |
//! | `riff pr open`, `riff pr wait`, `riff verify` | The steps of a pull request on GitHub, with `gh`. See [`pr`]. |
//! | `riff denials` | Counts the refusals of auto mode that the hook `PermissionDenied` posts to the riff. See [`denied`]. |
//! | `riff usage` | Shows the tokens and the models of an issue, of a wave, or of the sessions of this machine. See [`usage`]. |
//! | `riff audit` | Checks from the log and from `gh` that a wave followed the rules. See [`audit`]. |
//! | `riff cloud` | Makes and runs riff-server instances on Cloud Run, with `gcloud`. See [`cloud`]. |
//! | `riff init` | The first run of riff 3.0 on a machine: checks rootless podman, installs the daemon and pulls the session image. See [`first_run`]. |
//! | `riff image` | Prints the session image that the riff config of the repository pins. See [`session_image`]. |
//! | `riff run` | Runs one work item in a rootless podman container: the host claims, the container works. See [`run`]. |
//!
//! `riff --help` lists the commands under headings (see [`help`]).
//!
//! A session can leave the riff and join it again with the tools
//! `leave` and `join` (see [`leave`]).
//!
//! A granted claim and a new start show the earlier work on an item:
//! its pushed branch and its worktree (see [`dropped`]).
//!
//! ## The client of riff-server
//!
//! Each call to riff-server goes through the crate [`riff_client`]: the
//! sign-in, the tokens, the API calls and the watch stream. `riff` gives
//! it the parts of this machine with [`platform`]: the OS keyring, the
//! local folders and the lines on the terminal. The browser step of
//! `riff login` stays in [`login`].
//!
//! ## Secrets
//!
//! `riff` keeps the person tokens and the device key only in the OS
//! keyring, through [`riff_client::secrets`]. Each request with a token
//! carries a proof from the device key of the machine (see
//! [`riff_client::device`]). `riff relay mcp` and `riff relay watch` each hold a
//! session token in memory; it acts only as their session (see
//! [`riff_client::api`]). The tests give riff a home of
//! its own, with its secrets in files (see [`home`]).
//!
//! ## Messages are advice
//!
//! Each `read` result starts with [`text::DATA_NOTE`] (see [`text::inbox`]). It tells the agent
//! that only a verified message of the lead of its user counts as its
//! user. Each other message is advice (R10).

pub mod audit;
pub mod auto_update;
pub mod binary;
pub mod catch_up;
pub mod chat;
pub mod check;
pub mod claude_settings;
pub mod claude_token;
pub mod cloud;
pub mod compile_cache;
pub mod cycle;
pub mod daemon;
pub mod denied;
pub mod disk;
pub mod dropped;

pub mod first_run;
pub mod help;
pub mod home;

pub mod identity;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub mod keyring_store;
pub mod lead;
pub mod leave;
pub mod lifecycle;
pub mod local;
pub mod login;
pub mod look;
pub mod machine;
pub mod permissions;
pub mod platform;
pub mod tools;

#[cfg(target_os = "linux")]
pub mod podman;
pub mod pr;
pub mod release;
pub mod rollout;
pub mod room;
pub mod rule_sync;
pub mod run;
pub mod runtime;
pub mod sccache;
pub mod screen;
pub mod service;
pub mod session_image;
pub mod session_mcp;
pub mod session_relay;
pub mod settings;
pub mod smoke;
pub mod state;
pub mod style;
pub mod takeover;
pub mod temp;
pub mod text;
pub mod top;
pub mod update_steps;
pub mod usage;
pub mod view;
pub mod wave;
pub mod waves;
