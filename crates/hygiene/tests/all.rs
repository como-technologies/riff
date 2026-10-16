//! The integration tests of hygiene in one test binary: a build links
//! one binary, not one for each file. Each file is a module.

mod book;
mod ci;
mod ci_lock;
mod cli;
mod gate;
mod part;
mod stress;
mod test_files;
mod tracked;
