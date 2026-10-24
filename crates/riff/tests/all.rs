//! The integration tests of riff in one test binary: a build links
//! one binary, not one for each file. Each file is a module.
//! A file that sets the keyring store of the process is a binary of
//! its own: `back_in.rs`, `join_a_riff.rs`, `keyring_store.rs`. So is
//! `isolation.rs`: it listens on the port of the riff of the machine.

mod book;
mod common;

mod again;
mod audit;
mod auto_update;
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
mod daemon;
mod daemon_hosts;
mod first_run;
mod forge_text;
mod github;
mod help;
mod identity;
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
mod no_sign_in;
mod one_user;
mod owner;
mod pause;
mod pr;
mod pull_limit;
mod read_length;
mod release;
mod release_cycle;
mod restart;
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
mod usage;
mod wakes;
mod wave_conflict;
mod wave_stuck;
mod waves;
mod who;
mod wip;
