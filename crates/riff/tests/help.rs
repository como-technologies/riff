//! `riff --help` lists the commands that people use under headings,
//! each on one short line, and hides the plumbing
//! (01M3NJDSQ23FFRMH8ZD4GC57WY).

use crate::book;

use isolated::Isolated;

/// The headings of `riff --help`, in order.
const HEADINGS: [&str; 5] = [
    "Get started:",
    "Work in the riff:",
    "Pull requests:",
    "Lead:",
    "Members:",
];

/// The commands that only the plugin runs.
const PLUMBING: [&str; 4] = ["hook", "mcp", "statusline", "watch"];

/// The stdout of `riff ARGS`, in a terminal of 200 columns.
fn riff(args: &[&str]) -> String {
    let out = Isolated::shared()
        .riff()
        .args(args)
        .env("COLUMNS", "200")
        .output()
        .unwrap();
    assert!(out.status.success(), "riff {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The names of the commands under each heading of `help`, in order.
fn groups(help: &str) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut open = false;
    for line in help.lines() {
        if HEADINGS.contains(&line) {
            groups.push((line.to_owned(), Vec::new()));
            open = true;
        } else if open && line.starts_with("  ") {
            let name = line.split_whitespace().next().unwrap().to_owned();
            groups.last_mut().unwrap().1.push(name);
        } else {
            // A blank line or another heading ends the group.
            open = false;
        }
    }
    groups
}

#[test]
fn each_line_of_the_help_fits_in_80_columns() {
    for args in [
        &["--help"][..],
        &["-h"],
        &["help", "invite"],
        &["workers", "--help"],
        &["workers", "start", "--help"],
        &["update", "--help"],
        &["help", "server"],
        &["who", "-h"],
    ] {
        let help = riff(args);
        for line in help.lines() {
            assert!(line.chars().count() <= 80, "riff {args:?}: {line:?}");
        }
    }
}

#[test]
fn the_help_shows_the_groups_in_order() {
    let help = riff(&["--help"]);
    let groups = groups(&help);
    let headings: Vec<_> = groups.iter().map(|(h, _)| h.as_str()).collect();
    assert_eq!(headings, HEADINGS, "{help}");
    let names: Vec<_> = groups.iter().flat_map(|(_, n)| n.clone()).collect();
    for name in [
        "connect", "who", "claim", "pr", "verify", "workers", "invite",
    ] {
        assert!(names.contains(&name.to_owned()), "{name}: {help}");
    }
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "a command is in two groups");
    // Each command that `riff help` knows, and that is not plumbing,
    // is under a heading.
    for name in names {
        riff(&["help", &name]);
    }
    assert!(
        help.contains("  invite     Let a person join this riff\n"),
        "{help}"
    );
    assert!(
        help.contains("Run 'riff help <command>' for more"),
        "{help}"
    );
}

#[test]
fn the_help_hides_the_plumbing() {
    let help = riff(&["--help"]);
    for name in PLUMBING {
        assert!(
            !help
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{name} "))),
            "{name}: {help}"
        );
        assert!(!riff(&["help", name]).is_empty(), "riff help {name}");
    }
    assert!(riff(&["help", "hook"]).starts_with("Run a Claude Code hook\n"));
    let workers = riff(&["workers", "--help"]);
    assert!(!workers.contains("  run "), "{workers}");
    assert!(riff(&["workers", "help", "run"]).starts_with("Run CLAUDE as a worker"));
}

#[test]
fn riff_help_of_a_command_shows_the_long_text() {
    let help = riff(&["help", "invite"]);
    assert!(
        help.starts_with("Let a person join this riff\n\n"),
        "{help}"
    );
    assert!(
        help.contains("It adds their verified email to the members."),
        "{help}"
    );
    assert_eq!(riff(&["invite", "--help"]), help);
}

#[test]
fn the_help_shows_no_value_of_an_environment_variable() {
    let out = Isolated::shared()
        .riff()
        .arg("--help")
        .env("RIFF_SERVER", "http://secret.example:7878")
        .output()
        .unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    assert!(help.contains("RIFF_SERVER"), "{help}");
    assert!(!help.contains("secret.example"), "{help}");
}

#[test]
fn the_book_shows_how_to_find_a_command() {
    let part = book::part("how-it-works.md", "Find a command");
    let commands = book::commands_in(&part);
    assert_eq!(commands, ["riff --help", "riff help invite"]);
    let help = riff(&["--help"]);
    for heading in HEADINGS {
        let words = heading.trim_end_matches(':');
        assert!(part.contains(words), "{words} is not in the book");
        assert!(help.contains(heading));
    }
    for name in PLUMBING {
        assert!(part.contains(&format!("`riff {name}`")), "{name}");
    }
}

#[test]
fn the_server_help_is_short_and_riff_help_server_has_the_long_text() {
    let short = "--server <SERVER>  The riff-server (default: RIFF_SERVER, else";
    for args in [&["--help"][..], &["who", "-h"], &["workers", "start", "-h"]] {
        let help = riff(args);
        assert!(help.contains(short), "riff {args:?}: {help}");
        assert!(!help.contains("7878"), "riff {args:?}: {help}");
    }
    let long = riff(&["help", "server"]);
    for words in [
        "URL, HOST or HOST:PORT",
        "port 7878",
        "http://127.0.0.1:7878",
    ] {
        assert!(long.contains(words), "{words}: {long}");
    }
}

#[test]
fn a_usage_error_names_no_hidden_command() {
    // `riff` with no argument at all starts the riff (see start.rs).
    let out = Isolated::shared()
        .riff()
        .args(["--server", "127.0.0.1:9", "--color", "never", "--bad"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    for name in PLUMBING {
        assert!(!err.contains(name), "{name}: {err}");
    }
    assert!(!err.contains("[subcommands:"), "{err}");
}
