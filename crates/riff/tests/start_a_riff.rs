//! "Start a Riff", the first page of the book for a person: at most
//! three commands (R4), each one real, and no sign-in. "Add a Machine"
//! (R203): its commands are real too, and its update installs each
//! machine again. No book page names the Cloud Run URL (R5). The `riff-server` commands of the pages are checked in
//! `crates/riff-server/tests/start_a_riff.rs`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The commands in the `sh` blocks of the book page `name`, in order.
fn commands_of(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/src")
        .join(name);
    let page = fs::read_to_string(path).unwrap();
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && !line.is_empty() && !line.starts_with('#') {
            commands.push(line.to_owned());
        }
    }
    commands
}

/// The commands of "Start a Riff".
fn commands() -> Vec<String> {
    commands_of("start-a-riff.md")
}

/// Checks that each `riff` command in `commands` runs with `--help`.
fn each_is_real(commands: &[String]) {
    for command in commands {
        Command::cargo_bin("riff")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
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
    each_is_real(&riff);
}

#[test]
fn a_second_machine_installs_riff_names_the_first_and_connects() {
    let commands = commands_of("add-a-machine.md");
    let install: Vec<&str> = commands[1].split_whitespace().collect();
    assert_eq!(
        install.last(),
        Some(&env!("CARGO_PKG_NAME")),
        "{commands:?}"
    );
    assert!(install.contains(&env!("CARGO_PKG_REPOSITORY")));
    assert!(
        commands[2].contains("export RIFF_SERVER=http://FIRST:7878"),
        "{commands:?}"
    );
    let riff: Vec<String> = commands
        .iter()
        .filter(|c| c.starts_with("riff "))
        .cloned()
        .collect();
    assert_eq!(
        riff,
        [
            "riff connect claude",
            "riff who",
            "riff logout",
            "riff connect claude",
            "riff connect claude",
            "riff resume"
        ]
    );
    each_is_real(&riff);
}

#[test]
fn the_update_on_two_machines_does_the_install_of_each_machine_again() {
    let page = commands_of("add-a-machine.md");
    let installs: Vec<&String> = page
        .iter()
        .filter(|c| c.starts_with("cargo install "))
        .collect();
    // The second machine, then the update of the first and the second.
    assert_eq!(installs.len(), 3, "{page:?}");
    assert_eq!(installs[1], &commands()[0]);
    assert_eq!(installs[2], installs[0]);
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
