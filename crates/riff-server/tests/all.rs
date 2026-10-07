//! The integration tests of riff-server in one test binary: a build links
//! one binary, not one for each file. Each file is a module.

mod common;

mod auth;
mod build;
mod calls;
mod cli;
mod client_check;
mod cloud;
mod cut;
mod direct;
mod dpop;
mod facts;
mod format;
mod gcs;
mod help;
mod import;
mod lease;
mod log_lines;
mod log_tools;
mod me;
mod members;
mod revoke;
mod saved;
mod session;
mod sign_in;
mod signed;
mod start_a_riff;
mod start_a_team_riff;
mod start_order;
mod stop;
mod stream;
mod token;
mod trusted_riff;
