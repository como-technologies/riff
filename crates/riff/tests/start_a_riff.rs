//! "Start a Riff", the first page of the book for a person: a local riff
//! with at most three commands (R4), each one real, and no sign-in. A
//! riff with sign-in: `riff connect claude` signs in
//! (01M3JZN1ZZED3FXQEFNJ4KVCN5). "Add a Machine"
//! (R203): its commands are real too, and its update installs each
//! machine again. No book page names the Cloud Run URL (R5). The `riff-server` commands of the pages are checked in
//! `crates/riff-server/tests/start_a_riff.rs`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The text of the book page `name`.
fn page(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/src")
        .join(name);
    fs::read_to_string(path).unwrap()
}

/// The commands in the `sh` blocks of `text`, in order.
fn commands_in(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && !line.is_empty() && !line.starts_with('#') {
            commands.push(line.to_owned());
        }
    }
    commands
}

/// The commands of the book page `name`, in order.
fn commands_of(name: &str) -> Vec<String> {
    commands_in(&page(name))
}

/// The commands of one `##` part of the book page `name`.
fn commands_of_part(name: &str, heading: &str) -> Vec<String> {
    let page = page(name);
    let start = page
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("{heading} is in {name}"));
    let rest = &page[start + 1..];
    let part = rest[3..].find("\n## ").map_or(rest, |end| &rest[..end + 3]);
    commands_in(part)
}

/// The commands of "Start a local riff".
fn commands() -> Vec<String> {
    commands_of_part("start-a-riff.md", "Start a local riff")
}

#[test]
fn a_person_joins_a_riff_with_sign_in_with_riff_connect_claude() {
    let commands = commands_of_part("start-a-riff.md", "Join a riff with sign-in");
    assert_eq!(commands.len(), 3, "{commands:?}");
    assert!(
        commands[1].contains("export RIFF_SERVER=URL"),
        "{commands:?}"
    );
    assert_eq!(commands[2], "riff connect claude");
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
    let commands = commands_of_part("add-a-machine.md", "On the second machine");
    let install: Vec<&str> = commands[0].split_whitespace().collect();
    assert_eq!(
        install.last(),
        Some(&env!("CARGO_PKG_NAME")),
        "{commands:?}"
    );
    assert!(install.contains(&env!("CARGO_PKG_REPOSITORY")));
    assert!(
        commands[1].contains("export RIFF_SERVER=http://FIRST:7878"),
        "{commands:?}"
    );
    // The first machine gets the OAuth client of the person from the
    // environment (01M3JZN229S3YA3BR6GN5H3MTY).
    let first = commands_of_part("add-a-machine.md", "On the first machine");
    assert_eq!(
        first[0],
        "export RIFF_OIDC_CLIENT_ID=ID RIFF_OIDC_CLIENT_SECRET=SECRET"
    );
    let riff: Vec<String> = commands_of("add-a-machine.md")
        .iter()
        .filter(|c| c.starts_with("riff "))
        .cloned()
        .collect();
    assert_eq!(
        riff,
        [
            "riff login",
            "riff connect claude",
            "riff who",
            "riff update",
            "riff login",
            "riff update",
            "riff resume"
        ]
    );
    each_is_real(&riff);
}

/// Each machine updates with `riff update` (01M3K0Q892KWM76R9DJC1P37JA),
/// never with a `cargo install` of its own.
#[test]
fn the_update_on_two_machines_runs_riff_update_on_each_machine() {
    let update = commands_of_part("add-a-machine.md", "Update riff on two machines");
    let updates = update.iter().filter(|c| *c == "riff update").count();
    assert_eq!(updates, 2, "{update:?}");
    assert!(
        !update.iter().any(|c| c.starts_with("cargo install ")),
        "{update:?}"
    );
}

/// "Update riff" of "Start a Riff" is one command.
#[test]
fn a_person_updates_riff_with_riff_update() {
    let update = commands_of_part("start-a-riff.md", "Update riff");
    assert_eq!(update, ["riff update"]);
    each_is_real(&update);
}

/// "Start a Team Riff" (01M3MEFG6F102T1H8DFJ38EJ4A): the owner uses the
/// riff, signs in first, then invites each person. Each `riff` command
/// is real. The server part of the page runs in
/// `crates/riff-server/tests/start_a_team_riff.rs`.
#[test]
fn the_owner_of_a_team_riff_signs_in_then_invites() {
    let commands = commands_of("start-a-team-riff.md");
    assert!(
        commands.contains(&"echo 'export RIFF_SERVER=URL' >> ~/.bashrc".to_owned()),
        "{commands:?}"
    );
    let riff: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect();
    assert_eq!(riff, ["riff login", "riff invite EMAIL"]);
    each_is_real(&riff);
}

/// "Start a Riff" links the path for a team.
#[test]
fn start_a_riff_links_start_a_team_riff() {
    assert!(page("start-a-riff.md").contains("(start-a-team-riff.md)"));
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
