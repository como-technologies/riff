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

/// The commands that riff does not have (01M59BXNPR3AZ0HAS9GQCZ4PH2).
const REMOVED: [&str; 7] = [
    "connect",
    "enable",
    "disable",
    "watch",
    "hook",
    "statusline",
    "mcp",
];

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
    for name in ["setup", "who", "claim", "pr", "verify", "invite"] {
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
        help.contains("  invite        Let a person join this riff\n"),
        "{help}"
    );
    assert!(
        help.contains("Run 'riff help <command>' for more"),
        "{help}"
    );
}

#[test]
fn the_help_lists_none_of_the_removed_commands() {
    let help = riff(&["--help"]);
    for name in REMOVED {
        assert!(
            !help
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{name} "))),
            "{name}: {help}"
        );
        let out = Isolated::shared().riff().args([name]).output().unwrap();
        assert!(!out.status.success(), "riff {name} exists");
    }
}

#[test]
fn riff_help_of_a_command_shows_the_long_text() {
    let help = riff(&["help", "invite"]);
    assert!(
        help.starts_with("Let a person join this riff\n\n"),
        "{help}"
    );
    assert!(
        help.contains("It adds the verified email of a person to the members"),
        "{help}"
    );
    assert_eq!(riff(&["invite", "--help"]), help);
}

/// 01M53NGW6F5HP1DZ9QRGP50MGT: the help has no command and no flag of
/// the riff of peers. The member commands take the email as an argument,
/// with no flag `--email` (01M588J2QAF288ZB3M9PBN71ZS).
#[test]
fn the_help_has_no_command_and_no_flag_of_the_riff_of_peers() {
    let help = riff(&["--help"]);
    for name in ["join", "leave"] {
        assert!(
            !help
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{name} "))),
            "{name}: {help}"
        );
        let out = Isolated::shared()
            .riff()
            .args([name, "--help"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "riff {name} --help: {out:?}");
    }
    for (name, usage) in [
        ("invite", "Usage: riff invite [OPTIONS] <EMAIL>"),
        ("remove", "Usage: riff remove [OPTIONS] <EMAIL>"),
        ("members", "Usage: riff members [OPTIONS]"),
    ] {
        let help = riff(&[name, "--help"]);
        assert!(help.lines().any(|l| l == usage), "{help}");
        for word in [
            "USER",
            "--host",
            "--email",
            "key",
            "riff daemon",
            "invite string",
        ] {
            assert!(!help.contains(word), "riff {name} --help: {word}: {help}");
        }
    }
    for args in [&["run", "--help"][..], &["wave", "run", "--help"]] {
        let help = riff(args);
        assert!(!help.contains("--peers"), "riff {args:?}: {help}");
    }
    let help = riff(&["release", "--help"]);
    assert!(!help.contains("--forge"), "{help}");
}

/// 01M53NGW6F5HP1DZ9QRGP50MGT, 01M57D4FZGCVE62AP4MVG59HM4,
/// 01M53NGVE839TRH7XB7CDPQNZE: `riff view` shows the sessions of this
/// machine, or with `--host` of another host through riff-server, and
/// `riff lead start` asks the riff daemon to run the wave when none
/// runs.
#[test]
fn the_help_of_view_and_lead_start_says_what_they_do_in_riff_3() {
    // The words, with no line breaks of the wrap.
    let words = |args: &[&str]| riff(args).split_whitespace().collect::<Vec<_>>().join(" ");
    let view = words(&["view", "--help"]);
    assert!(
        view.contains("Show the riff daemon of HOST, through riff-server."),
        "{view}"
    );
    for word in ["other machines", "offline"] {
        assert!(!view.contains(word), "riff view --help: {word}: {view}");
    }
    let lead = words(&["lead", "start", "--help"]);
    assert!(
        lead.contains(
            "When no wave of this repository runs, riff first asks the riff daemon to run the wave"
        ),
        "{lead}"
    );
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
}

#[test]
fn the_server_help_is_short_and_riff_help_server_has_the_long_text() {
    let short = "--server <SERVER>  The riff-server (default: RIFF_SERVER, else";
    for args in [&["--help"][..], &["who", "-h"]] {
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
fn riff_with_no_command_names_no_hidden_command() {
    for args in [&[][..], &["--server", "127.0.0.1:9"]] {
        let out = Isolated::shared().riff().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "riff {args:?}: {out:?}");
        let err = String::from_utf8(out.stderr).unwrap();
        for name in REMOVED {
            assert!(!err.contains(name), "riff {args:?}: {name}: {err}");
        }
        assert!(!err.contains("[subcommands:"), "riff {args:?}: {err}");
    }
    let out = Isolated::shared().riff().output().unwrap();
    let help = String::from_utf8(out.stderr).unwrap();
    assert!(help.contains("Get started:\n"), "{help}");
    let out = Isolated::shared()
        .riff()
        .args(["--server", "127.0.0.1:9"])
        .output()
        .unwrap();
    let err = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        err,
        "error: name a command. Run 'riff --help' to list them.\n"
    );
}

/// The words of `text` on one line, with no backticks.
fn words(text: &str) -> String {
    let text = text.replace('`', "");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text under the heading `heading` of the book page `name`, up to
/// the next heading.
fn under(name: &str, heading: &str) -> String {
    let page = book::page(name);
    let start = page
        .lines()
        .position(|l| l == heading)
        .unwrap_or_else(|| panic!("{heading} is in {name}"));
    let rest: Vec<_> = page.lines().skip(start + 1).collect();
    let end = rest.iter().position(|l| l.starts_with("##"));
    rest[..end.unwrap_or(rest.len())].join("\n")
}

/// 01M588QXAGD3B8BTK7N17M29WM: the help of each `--host` flag that asks
/// the daemon of another host says who can use it.
/// The how-to of each flag in the book has the same words.
#[test]
fn the_help_and_the_book_say_who_can_use_each_host_flag() {
    let daemon = riff::text::DAEMON_HOST_WHO;
    for (args, who) in [
        (&["daemon", "status"][..], daemon),
        (&["daemon", "events"], daemon),
        (&["daemon", "limit"], daemon),
        (&["daemon", "cache"], daemon),
        (&["daemon", "sweep"], daemon),
        (&["daemon", "end"], daemon),
        (&["daemon", "restart"], daemon),
        (&["wave", "run"], daemon),
        (&["wave", "stop"], daemon),
        (&["view"], daemon),
    ] {
        for flag in ["--help", "-h"] {
            let help = words(&riff(&[args, &[flag]].concat()));
            // clap drops the period at the end of the help of a flag.
            let who = who.trim_end_matches('.');
            assert!(help.contains(who), "riff {args:?} {flag}: {help}");
        }
    }
    for (name, heading, who) in [
        (
            "development.md",
            "### Manage the daemon of another host",
            daemon,
        ),
        (
            "development.md",
            "### See a session on another host",
            daemon,
        ),
    ] {
        let text = words(&under(name, heading));
        assert!(text.contains(who), "{name} {heading}: {text}");
    }
}

/// 01M5GVQQ8NK3RD9CM3VTGXEHZQ: `riff blocked --help` and the description
/// of the `blocked` tool say who ends a block, as the book does: a prompt
/// of the person ends the block of the lead, and the block of a worker
/// ends at its next work after an answer.
#[test]
fn the_help_of_blocked_says_that_a_prompt_ends_only_the_block_of_the_lead() {
    let worker = "The block of a worker ends at its next work after an answer.";
    let lead = "The next prompt of the person ends the block of the lead.";
    let tool = words(&riff::tools::Tools::description("blocked").unwrap());
    // -h shows only the first line: the long help says who ends a block.
    let help = words(&riff(&["blocked", "--help"]));
    for text in [&help, &tool] {
        assert!(text.contains(worker), "{text}");
        assert!(text.contains(lead), "{text}");
        assert!(!text.contains("next prompt of your user"), "{text}");
    }
    let book = words(&under("how-it-works.md", "### End the wait of the lead"));
    assert!(book.contains("Give the lead a prompt"), "{book}");
}

#[test]
fn release_approve_help_says_who_may_approve() {
    let help = riff(&["release", "approve", "--help"]);
    for words in [
        "A person in a plain terminal may approve",
        "The lead of the owner or of an admin may approve too",
        "verified message",
        "says approve or deploy and names the version",
        "A worker and each other agent session may not",
    ] {
        let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains(words), "{words:?} missing in {help}");
    }
    assert!(!help.contains("it refuses in each agent"), "{help}");
}
