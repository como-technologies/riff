//! "Start a Riff", the first page of the book for a person: at most
//! three commands (R4), each one real, and no sign-in. No book page
//! names the Cloud Run URL (R5). The `riff-server` commands of the page
//! are checked in `crates/riff-server/tests/start_a_riff.rs`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The text of `docs/src/start-a-riff.md`.
fn page() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/start-a-riff.md");
    fs::read_to_string(path).unwrap()
}

/// The commands in the `sh` blocks of the page, in order.
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
fn a_person_starts_a_riff_with_at_most_three_commands() {
    let commands = commands();
    assert!(!commands.is_empty());
    assert!(commands.len() <= 3, "{commands:?}");
}

#[test]
fn the_install_command_installs_riff_and_its_server_from_the_repository() {
    let install = &commands()[0];
    let words: Vec<&str> = install.split_whitespace().collect();
    assert_eq!(words[..3], ["cargo", "install", "--locked"], "{install}");
    let git = words.iter().position(|w| *w == "--git").unwrap();
    assert_eq!(words[git + 1], env!("CARGO_PKG_REPOSITORY"));
    assert_eq!(words[git + 2..], [env!("CARGO_PKG_NAME"), "riff-server"]);
}

#[test]
fn each_riff_command_of_the_page_is_real_and_none_signs_in() {
    let riff: Vec<String> = commands()
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(riff, ["riff connect claude"]);
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
fn no_book_page_names_a_cloud_run_url() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src");
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(".run.app"), "{}", path.display());
    }
}
