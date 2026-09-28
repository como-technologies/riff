//! The `riff-server` commands of "Start a Riff" (R4) are real. The other
//! checks of the pages are in `crates/riff/tests/start_a_riff.rs` and
//! `crates/riff/tests/join_a_riff.rs`.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// The `riff-server` commands in the `sh` blocks of the book page `name`,
/// in order.
fn server_commands(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/src")
        .join(name);
    let page = fs::read_to_string(path).unwrap();
    let mut commands = Vec::new();
    let mut in_sh = false;
    for line in page.lines().map(str::trim) {
        if line.starts_with("```") {
            in_sh = !in_sh && line == "```sh";
        } else if in_sh && (line == "riff-server" || line.starts_with("riff-server ")) {
            commands.push(line.to_owned());
        }
    }
    commands
}

#[test]
fn the_page_runs_the_server_in_a_terminal() {
    // 01M3K0QM5HY852J4E5M2YQDYEM
    assert_eq!(server_commands("start-a-riff.md"), ["riff-server"]);
}

#[test]
fn each_riff_server_command_of_the_page_is_real() {
    for command in server_commands("start-a-riff.md") {
        Command::cargo_bin("riff-server")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
