//! "Start a Riff", the first page of the book for a person: a local riff
//! with at most three commands (R4), each one real, and no sign-in. A
//! riff with sign-in: `riff connect claude` signs in
//! (01M3JZN1ZZED3FXQEFNJ4KVCN5). No book page names the Cloud Run URL
//! (R5). The `riff-server` commands of the pages are checked in
//! `crates/riff-server/tests/start_a_riff.rs`. "Join a Riff" is checked
//! in `join_a_riff.rs`.

mod book;

use std::fs;

use book::{commands_of, commands_of_part, each_is_real, page};

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
    for entry in fs::read_dir(book::dir()).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains(".run.app"), "{}", path.display());
    }
}
