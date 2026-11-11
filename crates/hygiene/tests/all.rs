//! The integration tests of hygiene in one test binary: a build links
//! one binary, not one for each file. Each file is a module.

mod archive;
mod book;
mod build_once;
mod ci;
mod ci_lock;
mod ci_test;
mod cli;
mod dependabot;
mod gate;
mod merge_queue;
mod part;
mod stress;
mod terminal;
mod test_files;
mod tracked;
mod versions;
