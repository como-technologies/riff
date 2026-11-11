//! The integration tests of riff in one test binary: a build links
//! one binary, not one for each file. Each file is a module.
//! A file that sets the keyring store of the process is a binary of
//! its own: `back_in.rs`, `join_a_riff.rs`, `keyring_store.rs`. So is
//! `isolation.rs`: it listens on the port of the riff of the machine.
//! So is `restart.rs`: no test of its process starts a child
//! (01M752WCFNXA7PEWM6X4FVDVYH).
//!
//! A module whose tests need the daemon, `riff init` or a session runs on
//! Linux only: the daemon needs Linux (01M6YPB5B42FZP7GCDAVTN4PWY,
//! 01M6YPB5GTHFQG8HGM57DVBGBN).

mod book;
mod common;

mod again;
mod audit;
mod auto_update;
mod base_image;
mod broken_main;
mod build;
mod chat;
mod check_build;
mod claim;
mod claude_token;
mod cli;
mod closed_pipe;
mod cloud;
mod compile_cache;
mod conduct;
mod container_commands;
#[cfg(target_os = "linux")]
mod daemon;
#[cfg(target_os = "linux")]
mod daemon_hosts;
#[cfg(target_os = "linux")]
mod first_run;
mod forge_text;
mod gh_callers;
mod gh_costs;
mod gh_wrapper;
mod git_fault;
mod github;
mod github_apps;
mod guard;
mod help;
#[cfg(target_os = "linux")]
mod home_folders;
mod identity;
mod launchd;
mod lead_book;
mod lead_clone;
mod lead_container;
mod lead_move;
mod lead_notes;
mod lead_step;
mod lead_whoami;
mod leave;
mod lifecycle;
mod link;
mod local_mode;
mod login;
mod logout;
mod look;
#[path = "loop.rs"]
mod loops;
mod mcp;
mod measure;
mod members;
#[cfg(target_os = "linux")]
mod network;
mod no_sign_in;
mod no_skill;
mod one_user;
mod owner;
mod package_sources;
mod pause;
mod pr;
#[cfg(target_os = "linux")]
mod ps;
mod pull_limit;
mod questions;
mod read_length;
mod release;
mod release_cycle;
#[cfg(target_os = "linux")]
mod run;
mod save;
mod secrets;
mod session;
mod session_image;
mod setup;
mod signed;
mod signed_server;
mod smoke;
mod start_a_riff;
mod states;
mod streams;
mod tail;
mod token_calls;
mod top;
mod top_fault;
mod trusted_riff;
mod tui;
mod usage;
mod wakes;
mod wave_branch;
mod wave_checks;
mod wave_conflict;
mod wave_done_when;
mod wave_idle;
mod wave_list;
mod wave_merged;
mod wave_order;
mod wave_question;
mod wave_queue;
mod wave_stuck;
mod waves;
mod who;
mod wip;
