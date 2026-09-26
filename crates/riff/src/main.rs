//! The local client that finds sessions and wakes yours.
//!
//! This is a placeholder. The design is still open; see the book.

use clap::Parser;

/// The local client that finds sessions and wakes yours.
#[derive(Parser)]
#[command(version, about)]
struct Cli {}

fn main() {
    Cli::parse();
    println!("riff: nothing to do yet. See the book for the design status.");
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
