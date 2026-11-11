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

/// #1643: `riff --help` lists `join` and `leave`. `riff wave run`, `riff
/// wave stop` and `riff daemon host` are gone, with no shim: the help of
/// `riff wave` and of `riff daemon` lists none of them, and each one fails.
#[test]
fn the_help_lists_join_and_leave_and_not_wave_run_wave_stop_or_daemon_host() {
    let help = riff(&["--help"]);
    for line in [
        "  join          Give workers to this repository from this host\n",
        "  leave         Stop the workers of this host in this repository\n",
    ] {
        assert!(help.contains(line), "{line}: {help}");
    }
    let wave = riff(&["wave", "--help"]);
    let daemon = riff(&["daemon", "--help"]);
    for (parent, help, gone) in [
        ("wave", &wave, ["run", "stop"]),
        ("daemon", &daemon, ["host", "host"]),
    ] {
        for name in gone {
            assert!(
                !help
                    .lines()
                    .any(|l| l.trim_start().starts_with(&format!("{name} "))),
                "riff {parent} {name}: {help}"
            );
            let out = Isolated::shared()
                .riff()
                .args([parent, name])
                .output()
                .unwrap();
            assert!(!out.status.success(), "riff {parent} {name} exists");
        }
    }
    // The help of the two commands says what they do.
    let join = words(&riff(&["join", "--help"]));
    for word in ["--host <HOST>", "part workers", "riff leave"] {
        assert!(join.contains(word), "{word}: {join}");
    }
    let leave = words(&riff(&["leave", "--help"]));
    for word in ["--host <HOST>", "--now", "riff join"] {
        assert!(leave.contains(word), "{word}: {leave}");
    }
}

/// #1644: `riff lead --help` lists `stop`, with `start`, `move` and
/// `notes`. The help of `riff lead stop` says what it does, and
/// `riff daemon end` names it in its help.
#[test]
fn the_help_of_lead_lists_stop() {
    let help = riff(&["lead", "--help"]);
    for line in [
        "  start  Start the lead of this repository in a container\n",
        "  stop   Stop the lead of this repository, and end its container\n",
        "  move   Move the lead of this repository to another host\n",
    ] {
        assert!(help.contains(line), "{line}: {help}");
    }
    let stop = words(&riff(&["lead", "stop", "--help"]));
    for word in [
        "--host <HOST>",
        "write its notes",
        "part lead",
        "riff lead start",
    ] {
        assert!(stop.contains(word), "{word}: {stop}");
    }
    let start = words(&riff(&["lead", "start", "--help"]));
    for word in ["starts no workers", "riff join"] {
        assert!(start.contains(word), "{word}: {start}");
    }
    let end = words(&riff(&["daemon", "end", "--help"]));
    assert!(end.contains("stop it with riff lead stop"), "{end}");
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
    for args in [&["run", "--help"][..], &["join", "--help"]] {
        let help = riff(args);
        assert!(!help.contains("--peers"), "riff {args:?}: {help}");
    }
    let help = riff(&["release", "--help"]);
    assert!(!help.contains("--forge"), "{help}");
}

/// 01M6VGHPP3ZT5TTMSGAXV3ZK7W, 01M6QVSJC4V3Z3NB6MJW4XGJSR: riff has no
/// command `riff view`, also not with `--host`, and `riff --help` and
/// `riff lead start --help` name none. `riff lead start` asks the riff
/// daemon for no wave.
#[test]
fn riff_view_is_gone_and_the_help_of_lead_start_says_what_it_does() {
    // The words, with no line breaks of the wrap.
    let words = |args: &[&str]| riff(args).split_whitespace().collect::<Vec<_>>().join(" ");
    for args in [&["view"][..], &["view", "--host", "thelio"]] {
        let out = Isolated::shared().riff().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "riff {args:?}: {out:?}");
    }
    let help = riff(&["--help"]);
    assert!(
        !help.lines().any(|l| l.trim_start().starts_with("view ")),
        "{help}"
    );
    assert!(!help.contains("riff view"), "{help}");
    let lead = words(&["lead", "start", "--help"]);
    assert!(
        lead.contains("starts no workers and does not join the host"),
        "{lead}"
    );
    assert!(!lead.contains("run the wave"), "{lead}");
    assert!(!lead.contains("riff view"), "{lead}");
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

/// 01M6VAZ7WH3MFAYF4PAZW6EDVY: with no terminal, `riff` with no
/// command prints the help on stderr and exits with 2, also with only
/// the global options. The help names no hidden command.
#[test]
fn riff_with_no_command_and_no_terminal_prints_the_help() {
    let help = riff(&["--help"]);
    for args in [&[][..], &["--server", "127.0.0.1:9"]] {
        let out = Isolated::shared().riff().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "riff {args:?}: {out:?}");
        assert!(out.stdout.is_empty(), "riff {args:?}: {out:?}");
        let err = String::from_utf8(out.stderr).unwrap();
        for name in REMOVED {
            assert!(!err.contains(name), "riff {args:?}: {name}: {err}");
        }
        assert!(!err.contains("[subcommands:"), "riff {args:?}: {err}");
        assert!(err.contains("Get started:\n"), "riff {args:?}: {err}");
        assert_eq!(err.lines().count(), help.lines().count(), "{err}");
    }
}

/// 01M54216E2GY49GFV5SNPWTBEB: `riff --help` says that `riff` with no
/// command starts the TUI.
#[test]
fn the_help_says_that_riff_with_no_command_starts_the_tui() {
    for args in [&["--help"][..], &["-h"], &["help"]] {
        let help = riff(args);
        assert!(
            help.contains(
                "\nUsage: riff [OPTIONS] [COMMAND]\n\n\
                 With no command, in a terminal, riff starts the TUI.\n\nGet started:\n"
            ),
            "riff {args:?}: {help}"
        );
    }
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
        (&["join"], daemon),
        (&["leave"], daemon),
        (&["lead", "stop"], daemon),
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
        ("development.md", "### Add a host to the wave", daemon),
        ("start-a-riff.md", "### Stop the lead", daemon),
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

/// 01M6WZ40SFVZ6CRQ1H4Z27CWP4: only a person approves a deploy. The help
/// of `riff release approve` says so, and it names no approval of the
/// lead.
#[test]
fn release_approve_help_says_who_may_approve() {
    let help = riff(&["release", "approve", "--help"]);
    let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "Only a person in a plain terminal approves",
        "It refuses each agent session: a worker, a container session, and the lead, also \
         when the owner said approve in riff chat",
    ] {
        assert!(flat.contains(words), "{words:?} missing in {help}");
    }
    for words in ["may approve too", "verified message", "asks no yes or no"] {
        assert!(!flat.contains(words), "{words:?} in {help}");
    }
}

/// 01M6WZ40VYDBBTJBTQEBY2984P: the help of `riff release publish` says
/// that the ruleset `releases` decides the push of the tag, and asks for
/// no `gh` of an admin.
#[test]
fn release_publish_help_says_that_the_ruleset_decides_the_tag() {
    let help = riff(&["release", "publish", "--help"]);
    let flat = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(
            "The ruleset releases of GitHub decides the push of the tag: it lets an admin of \
             the repository and the lead App push it, so the lead pushes it from its container."
        ),
        "{help}"
    );
    assert!(!flat.contains("with a gh of an admin"), "{help}");
}

/// 01M5SMPX8NM75P8E38DGQ17W7B: the release gate of `riff wave end`
/// holds only in a repository with `[release]` in `riff.toml`. The help
/// and "Turn on the release cycle" in the book say so, and the help does
/// not name `deploy/release-check.sh`: `riff release cut` runs that
/// script, and `riff wave end` does not look for it. The book says the
/// rule in that one place only.
#[test]
fn the_help_of_wave_end_names_the_release_table_of_riff_toml() {
    let help = words(&riff(&["wave", "end", "--help"]));
    let gate = "In a repository with [release] in riff.toml, it also refuses \
                while a merged pull request of the wave is in no checked release";
    assert!(help.contains(gate), "no {gate:?} in {help}");
    assert!(!help.contains("release-check.sh"), "{help}");

    let book = words(&under("development.md", "#### Turn on the release cycle"));
    let said = "In a repository with [release] in riff.toml, riff wave end also \
                refuses while a merged pull request of the wave is in no checked release.";
    assert!(book.contains(said), "no {said:?} in {book}");

    // The rule is in one place only. The intro of "The release cycle" and
    // "End the wave of the release" link to it.
    let page = words(&book::page("development.md"));
    let gate = "is in no checked release";
    assert_eq!(page.matches(gate).count(), 1, "the rule is in one place");
    for heading in ["### The release cycle", "#### End the wave of the release"] {
        let section = under("development.md", heading);
        assert!(
            section.contains("](#turn-on-the-release-cycle)"),
            "no link to the rule in {heading}: {section}"
        );
        assert!(!words(&section).contains(gate), "{heading}: {section}");
    }

    // The help of the cut still says that the cut runs the script.
    let cut = words(&riff(&["release", "cut", "--help"]));
    assert!(cut.contains("runs deploy/release-check.sh"), "{cut}");
    // 01M6ZM68Y1QCTA2M8DHYRMKBPN: the help and the how-to of the cut say
    // that it checks the alerts.
    let alerts = "while an alert of the repository has no review";
    assert!(cut.contains(alerts), "{cut}");
    let how_to = words(&under(
        "development.md",
        "#### Cut the release pull request",
    ));
    assert!(how_to.contains(alerts), "{how_to}");
    // 01M748V0HMV7D9T4F6NCB7HQHG: they name the two closed alerts that
    // need no comment.
    assert!(
        cut.contains(
            "A revoked secret and a Dependabot alert that a rule dismissed need no comment"
        ),
        "{cut}"
    );
    for case in [
        "a secret scanning alert closed as revoked",
        "a Dependabot alert that a rule of GitHub dismissed (auto-dismissed)",
    ] {
        assert!(how_to.contains(case), "{case} in {how_to}");
    }
}
