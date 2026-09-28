//! The text and the commands of the book pages, for the page tests.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// The directory of the book pages.
pub fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src")
}

/// The text of the book page `name`.
pub fn page(name: &str) -> String {
    fs::read_to_string(dir().join(name)).unwrap()
}

/// The commands in the `sh` blocks of `text`, in order.
pub fn commands_in(text: &str) -> Vec<String> {
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
pub fn commands_of(name: &str) -> Vec<String> {
    commands_in(&page(name))
}

/// The commands of one `##` part of the book page `name`.
pub fn commands_of_part(name: &str, heading: &str) -> Vec<String> {
    let page = page(name);
    let start = page
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("{heading} is in {name}"));
    let rest = &page[start + 1..];
    let part = rest[3..].find("\n## ").map_or(rest, |end| &rest[..end + 3]);
    commands_in(part)
}

/// The `riff` commands of the book page `name`, in order.
pub fn riff_commands_of(name: &str) -> Vec<String> {
    commands_of(name)
        .into_iter()
        .filter(|c| c.starts_with("riff "))
        .collect()
}

/// Checks that each `riff` command in `commands` runs with `--help`.
pub fn each_is_real(commands: &[String]) {
    for command in commands {
        Command::cargo_bin("riff")
            .unwrap()
            .args(command.split_whitespace().skip(1))
            .arg("--help")
            .assert()
            .success();
    }
}
