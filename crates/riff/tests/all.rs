//! The integration tests of riff in one test binary: a build links
//! one binary, not one for each file. Each file is a module.
//! A file that sets the keyring store of the process is a binary of
//! its own: `back_in.rs`, `join_a_riff.rs`. So is `isolation.rs`: it
//! listens on the port of the riff of the machine.

mod book;
mod common;

mod again;
mod audit;
mod auto_update;
mod behind;
mod build;
mod chat;
mod claim;
mod clear;
mod cli;
mod cloud;
mod compact;
mod conduct;
mod dev;
mod enable;
mod end;
mod forge_text;
mod fresh;
mod github;
mod help;
mod hosts;
mod hygiene;
mod identity;
mod jobserver;
mod lead_book;
mod lead_step;
mod leave;
mod left;
mod lifecycle;
mod limits;
mod link;
mod link_limits;
mod linked;
mod login;
mod logout;
mod look;
#[path = "loop.rs"]
mod loops;
mod mcp;
mod measure;
mod members;
mod monitor;
mod new_machine;
mod next;
mod no_sign_in;
mod one_user;
mod over_limit;
mod owner;
mod pause;
mod plugin;
mod pr;
mod read_length;
mod restart;
mod rollout;
mod secrets;
mod session;
mod setup;
mod signed;
mod smoke;
mod start;
mod start_a_riff;
mod states;
mod statusline;
mod streams;
mod tail;
mod terminal;
mod tidy;
mod token_calls;
mod top;
mod top_fault;
mod trusted_riff;
mod usage;
mod wakes;
mod watch_limit;
mod waves;
mod who;
mod wip;
mod worker;
mod workloads;
