//! The Sub-Etha: the central service that sessions connect to.
//!
//! This is a placeholder. The design is still open; see the book.

use clap::Parser;

/// The Sub-Etha: the central service that sessions connect to.
#[derive(Parser)]
#[command(version, about)]
struct Cli {}

fn main() {
    Cli::parse();
    println!("subetha: nothing to do yet. See the book for the design status.");
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
