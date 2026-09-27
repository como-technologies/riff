//! The quick start of the book ("Join In"): at most three commands
//! (R4), each one real, and the shared server as the default (R133).

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The text of `docs/src/quick-start.md`.
fn page() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/quick-start.md");
    fs::read_to_string(path).unwrap()
}

/// The commands in the `sh` blocks of the quick start, in order.
fn commands() -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page().lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && !line.is_empty() && !line.starts_with('#') {
            commands.push(line.to_owned());
        }
    }
    commands
}

#[test]
fn a_person_joins_with_at_most_three_commands() {
    let commands = commands();
    assert!(!commands.is_empty());
    assert!(commands.len() <= 3, "{commands:?}");
}

#[test]
fn the_install_command_installs_riff_from_its_repository() {
    let install = &commands()[0];
    let words: Vec<&str> = install.split_whitespace().collect();
    assert_eq!(words[..3], ["cargo", "install", "--locked"], "{install}");
    let git = words.iter().position(|w| *w == "--git").unwrap();
    assert_eq!(words[git + 1], env!("CARGO_PKG_REPOSITORY"));
    assert_eq!(words.last(), Some(&env!("CARGO_PKG_NAME")));
}

#[test]
fn each_riff_command_of_the_quick_start_is_real() {
    let riff: Vec<String> = commands()
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(riff, ["riff login", "riff connect claude"]);
    for command in riff {
        Command::cargo_bin("riff")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}

#[test]
fn the_quick_start_names_the_default_server() {
    assert!(page().contains(riff::api::DEFAULT_SERVER));
}
