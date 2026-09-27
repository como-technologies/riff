//! The `riff-server` commands of "Start a Riff" (R4) are real. The
//! other checks of the page are in `crates/riff/tests/start_a_riff.rs`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The `riff-server` commands in the `sh` blocks of the page, in order.
fn server_commands() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/start-a-riff.md");
    let page = fs::read_to_string(path).unwrap();
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && line.starts_with("riff-server ") {
            commands.push(line.to_owned());
        }
    }
    commands
}

#[test]
fn the_page_starts_the_server_as_a_service() {
    assert_eq!(server_commands(), ["riff-server install"]);
}

#[test]
fn each_riff_server_command_of_the_page_is_real() {
    for command in server_commands() {
        Command::cargo_bin("riff-server")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
