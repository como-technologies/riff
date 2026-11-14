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
//! | `riff device add --key NAME`, `riff device list`, `riff device remove NAME` | Add, list and remove the FIDO2 / WebAuthn authenticators of the person. See [`device`]. |
//! | `riff release test`, `cut`, `verify`, `status`, `approve`, `hold` | The release cycle: the real-session test, the tag, the GitHub release, the pull request of the next version, the approval of the deploy, and the hold of a lead. See [`cycle`], [`hold`], and [`alerts`] for the alert gate of the cut. |
//! | `riff pr open`, `riff pr wait`, `riff verify` | The steps of a pull request on GitHub, with `gh`. See [`pr`]. |
//! | `riff council list` | Lists the members of the council of a verify: the files of `.riff/council/` over the default set that riff builds in. See [`council`]. |
//! | `riff council show PR` | Shows the council of a pull request: the members that join for its files and its labels, from the member files of its base branch. See [`council::pick`]. |
//! | `riff session why` | Shows why a session of a wave ended with no pull request. See [`ends`]. |
//! | `riff denials` | Counts the refusals of auto mode that the hook `PermissionDenied` posts to the riff. See [`denied`]. |
//! | `riff usage` | Shows the tokens and the models of an issue, of a wave, or of the sessions of this machine. See [`usage`]. |
//! | `riff audit` | Checks from the log and from `gh` that a wave followed the rules. See [`audit`]. |
//! | `riff cloud` | Makes and runs riff-server instances on Cloud Run, with `gcloud`. See [`cloud`]. |
//! | `riff init` | The first run of riff on a machine: checks rootless podman, installs the daemon and pulls the session image. See [`first_run`]. |
//! | `riff project init`, `riff project list`, `riff project remove` | The projects of the riff: checks the repository on GitHub, sets it up for riff and enrolls it, lists the projects, and forgets one. See [`project`], [`repo_setup`]. |
//! | `riff image` | Prints the session image that the riff config of the repository pins. With `--check`, it asks the registry whether a host with no login can pull it. See [`session_image`], [`registry`]. |
//! | `riff run` | Runs one work item in a rootless podman container: the host claims, the container works. See [`run`]. |
//! | `riff ps` | Shows the riff processes of this host as a tree: the daemon, each session container and each riff command. See [`ps`]. |
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
//! that only a verified message of the lead of its project counts as its
//! user. Each other message is advice (R10).

pub mod alerts;
pub mod audit;
pub mod auto_update;
pub mod binary;
pub mod catch_up;
pub mod changes;
pub mod chat;
pub mod check;
pub mod claude_settings;
pub mod claude_token;
pub mod cloud;
pub mod compile_cache;
pub mod council;
pub mod cycle;
pub mod daemon;
pub mod denied;
pub mod device;
pub mod disk;
pub mod dropped;
pub mod ends;

pub mod first_run;
pub mod followers;
pub mod gh_wrapper;
pub mod ghcalls;
pub mod git_credential;
pub mod github;
pub mod guard;
pub mod help;
pub mod highlight;
pub mod hold;
pub mod home;
pub mod hooks;
pub mod hygiene_rules;

pub mod identity;
pub mod image_tools;
pub mod kept;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub mod keyring_store;
pub mod lead;
pub mod lead_host;
pub mod leave;
pub mod lifecycle;
pub mod local;
pub mod login;
pub mod look;
pub mod machine;
pub mod markdown;
pub mod order;
pub mod permissions;
pub mod platform;
pub mod tools;

#[cfg(target_os = "linux")]
pub mod podman;
pub mod pr;
pub mod project;
pub mod proof;
pub mod ps;
pub mod questions;
pub mod reach;
pub mod registry;
pub mod release;
pub mod release_config;
pub mod repo_setup;
pub mod rollout;
pub mod room;
pub mod run;
pub mod runtime;
pub mod sccache;
pub mod service;
pub mod session_image;
pub mod session_mcp;
pub mod session_relay;
pub mod session_setup;
pub mod settings;
pub mod smoke;
pub mod state;
pub mod style;
pub mod takeover;
pub mod temp;
pub mod text;
pub mod top;
pub mod tui;
pub mod typeahead;
pub mod update_steps;
pub mod usage;
pub mod view;
pub mod watches;
pub mod wave;
pub mod wave_list;
pub mod waves;
pub mod workflow;
