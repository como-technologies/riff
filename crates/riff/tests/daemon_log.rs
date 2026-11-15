//! #2156: `riff daemon log` on the host reads the log files of the riff
//! daemon itself, in the folder of the daemon: the lines since a time,
//! with a text, the newest ones, with no secret. No daemon runs in these
//! tests. The way through the socket of the lead and through riff-server
//! has its tests in `lead_container.rs` and in the daemon.

use std::process::Output;

use chrono::{DateTime, Duration, Utc};
use isolated::Isolated;

use crate::book;

/// The line of the log at `at` with `text`, as the daemon writes it.
fn line(at: DateTime<Utc>, text: &str) -> String {
    format!("{} {text}", at.format("%Y-%m-%dT%H:%M:%SZ"))
}

/// Writes the log files of the daemon in `env`: `old`, then `new`.
fn write_log(env: &Isolated, old: &[String], new: &[String]) {
    let dir = env.riff_home().join("daemon");
    std::fs::create_dir_all(&dir).unwrap();
    let file = |lines: &[String]| lines.iter().map(|l| format!("{l}\n")).collect::<String>();
    std::fs::write(dir.join(riff::daemon::log::OLD), file(old)).unwrap();
    std::fs::write(dir.join(riff::daemon::log::FILE), file(new)).unwrap();
}

/// `riff daemon log ARGS` in `env`, in the time zone UTC, with the value
/// of a secret variable of the host.
fn daemon_log(env: &Isolated, args: &[&str]) -> Output {
    daemon_log_in(env, "UTC", args)
}

/// `riff daemon log ARGS` in `env`, in the time zone `tz`, with the value
/// of a secret variable of the host.
fn daemon_log_in(env: &Isolated, tz: &str, args: &[&str]) -> Output {
    env.riff()
        .args(["daemon", "log"])
        .args(args)
        .env("TZ", tz)
        .env("GH_TOKEN", "a-token-of-the-host-1")
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// #2156, 01M7EX1PVSG64G8EE5SNDRHM7G: `--since` with a time of the day
/// or with a date and its zone, and `--grep`, pick the known lines of
/// both files, in their order. `--lines` keeps the newest, and says how
/// many more match.
#[test]
fn since_and_grep_pick_the_known_lines_of_the_log() {
    let env = Isolated::new();
    let now = Utc::now();
    let ago = |minutes| now - Duration::minutes(minutes);
    let old = [
        line(ago(180), "riff daemon: o/r: claims issue-12"),
        line(ago(120), "riff daemon: o/r: claims issue-13"),
    ];
    let new = [
        line(
            ago(60),
            "riff daemon: issue-12 ends: the cause: a test failed",
        ),
        line(ago(30), "riff daemon: o/r: claims issue-14"),
        line(ago(10), "riff daemon: issue-12 starts again"),
    ];
    write_log(&env, &old, &new);

    // A time of the day: the last one before now, in the local time.
    let since = ago(90).format("%H:%M").to_string();
    let out = daemon_log(&env, &["--since", &since, "--grep", "issue-12"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), format!("{}\n{}\n", new[0], new[2]));

    // A date and a time with its zone.
    let since = ago(150).format("%Y-%m-%dT%H:%MZ").to_string();
    let args = [
        "--since", &since, "--grep", "issue-13", "--grep", "issue-14",
    ];
    let out = daemon_log(&env, &args);
    assert_eq!(text(&out.stdout), format!("{}\n{}\n", old[1], new[1]));

    // The newest line, and the count of the older ones.
    let out = daemon_log(&env, &["--lines", "1"]);
    let found = riff::daemon::log::Found {
        lines: vec![new[2].clone()],
        more: 4,
    };
    assert_eq!(
        text(&out.stdout),
        format!("{}\n", riff::text::daemon_log(&found))
    );
    assert!(
        text(&out.stdout).starts_with("riff daemon log: 4 older lines match too."),
        "{}",
        text(&out.stdout)
    );

    // No line matches (01M7F0HEA4GVKQ47Q5GN29FQ22).
    let out = daemon_log(&env, &["--grep", "issue-99"]);
    assert!(out.status.success());
    assert_eq!(
        text(&out.stdout),
        "riff daemon log: no line of the log of the riff daemon matches.\n"
    );
}

/// #2156, 01M7EX1PVSG64G8EE5SNDRHM7G: with no `--lines`, riff shows the
/// newest 200 lines, and its first line says that one older line
/// matches too.
#[test]
fn with_no_lines_flag_riff_shows_the_newest_200_lines() {
    let env = Isolated::new();
    let at = Utc::now() - Duration::minutes(5);
    let lines: Vec<String> = (0..201)
        .map(|n| line(at, &format!("riff daemon: line {n}")))
        .collect();
    write_log(&env, &[], &lines);
    let out = daemon_log(&env, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    let mut shown = shown.lines();
    assert_eq!(
        shown.next(),
        Some(
            "riff daemon log: 1 older line matches too. Ask for more with --lines, up to 2000, \
             or for fewer with --since or --grep."
        )
    );
    assert_eq!(shown.collect::<Vec<_>>(), lines[1..]);
}

/// #2156, 01M7EX1PVSG64G8EE5SNDRHM7G: a time of the day is in the local
/// time of the machine where riff runs. At UTC+05:30, the time of the day
/// of 90 minutes ago picks the line of 60 minutes ago, and not the line
/// of 120 minutes ago.
#[test]
fn a_time_of_the_day_is_in_the_local_time() {
    let env = Isolated::new();
    let now = Utc::now();
    let ago = |minutes| now - Duration::minutes(minutes);
    let new = [
        line(ago(120), "riff daemon: o/r: claims issue-12"),
        line(ago(60), "riff daemon: o/r: claims issue-13"),
    ];
    write_log(&env, &[], &new);
    let local = (ago(90) + Duration::minutes(330))
        .format("%H:%M")
        .to_string();
    let out = daemon_log_in(&env, "IST-5:30", &["--since", &local]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), format!("{}\n", new[1]));
}

/// #2156, 01M7EX1Q1F468NWGPJJS0W0ZG9, 01M7F0HECNG3F2H1ZF4JB7WN0Y: a line
/// with a token-like text shows with `[secret]` in its place, and
/// `--grep` finds no part of a secret: a token of GitHub, a key of
/// Anthropic, `NAME=value` of a token, a bearer token, the value of a
/// secret variable of the host, each value of the `headers` and the
/// `env` of an MCP server of the host, the token of `riff claude-token`
/// in the keyring and the tokens of the sign-in file of claude, as in the
/// events of a session. When riff cannot read the MCP servers, it shows
/// no line, and exits with 1.
#[test]
fn a_line_with_a_token_like_text_is_filtered() {
    let env = Isolated::new();
    let secrets = env.riff_home().join("secrets");
    riff_client::secrets::file_set(&secrets, riff::claude_token::NAME, "a-keyring-token-3")
        .unwrap();
    let claude = env.home().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"an-access-token-4","expiresAt":0}}"#,
    )
    .unwrap();
    let at = Utc::now() - Duration::minutes(5);
    let lines = [
        line(
            at,
            "riff daemon: git push with ghs_16C7e42F292c6912E7710c838347Ae178B4a",
        ),
        line(
            at,
            r#"riff daemon: claude said {"token":"sk-ant-oat01-Ab_c-9"}"#,
        ),
        line(
            at,
            "riff daemon: podman run -e GH_TOKEN=abc -e RIFF_ITEM=issue-12",
        ),
        line(at, "riff daemon: Authorization: Bearer xyz"),
        line(
            at,
            "riff daemon: the token a-token-of-the-host-1 of the host",
        ),
        line(
            at,
            "riff daemon: docs refused docs-header-value-1, db refused db-env-value-2",
        ),
        line(
            at,
            "riff daemon: claude refused a-keyring-token-3 and an-access-token-4",
        ),
    ];
    write_log(&env, &[], &lines);
    let servers = env.riff_home().join("daemon").join(riff::session_mcp::FILE);
    std::fs::write(
        &servers,
        r#"{"mcpServers": {"docs": {"url": "u", "headers": {"X-Docs": "docs-header-value-1"}},
            "db": {"command": "db", "env": {"DB_AUTH": "db-env-value-2"}}}}"#,
    )
    .unwrap();
    let out = daemon_log(&env, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    let stamp = at.format("%Y-%m-%dT%H:%M:%SZ");
    assert_eq!(
        shown,
        format!(
            "{stamp} riff daemon: git push with [secret]\n\
             {stamp} riff daemon: claude said {{\"token\":\"[secret]\"}}\n\
             {stamp} riff daemon: podman run -e GH_TOKEN=[secret] -e RIFF_ITEM=issue-12\n\
             {stamp} riff daemon: Authorization: Bearer [secret]\n\
             {stamp} riff daemon: the token [secret] of the host\n\
             {stamp} riff daemon: docs refused [secret], db refused [secret]\n\
             {stamp} riff daemon: claude refused [secret] and [secret]\n"
        )
    );
    for probe in [
        "ghs_16C7",
        "sk-ant-oat01",
        "GH_TOKEN=abc",
        "Bearer xyz",
        "a-token-of",
        "docs-header",
        "db-env",
        "keyring-token",
        "access-token",
    ] {
        let out = daemon_log(&env, &["--grep", probe]);
        assert_eq!(
            text(&out.stdout),
            "riff daemon log: no line of the log of the riff daemon matches.\n",
            "{probe}"
        );
    }
    std::fs::write(&servers, "{").unwrap();
    let out = daemon_log(&env, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert!(
        text(&out.stderr).starts_with(riff::text::DAEMON_LOG_NO_MCP),
        "{}",
        text(&out.stderr)
    );
}

/// #2156, 01M7EX1PVSG64G8EE5SNDRHM7G, 01M7F0HE514TQMGQJKH8CVYY52: with
/// no log file, riff names the file and the command of the service
/// manager that shows the log, and exits with 1. A bad flag says what it
/// takes.
#[test]
fn with_no_log_file_riff_names_the_file_and_exits_with_1() {
    let env = Isolated::new();
    let out = daemon_log(&env, &[]);
    assert_eq!(out.status.code(), Some(1));
    let file = env.riff_home().join("daemon").join(riff::daemon::log::FILE);
    assert!(
        text(&out.stderr).contains(&format!("has no log file {}.", file.display())),
        "{}",
        text(&out.stderr)
    );
    let command = if cfg!(target_os = "macos") {
        "See its log with: tail -f "
    } else {
        "See its log with: journalctl --user -u riff\n"
    };
    assert!(text(&out.stderr).contains(command), "{}", text(&out.stderr));
    for (args, says) in [
        (
            &["--since", "noon"][..],
            riff::text::daemon_log_bad_since("noon"),
        ),
        (
            &["--since", "+10000-01-01T00:00Z"],
            riff::text::DAEMON_LOG_SINCE_RANGE.to_owned(),
        ),
        (&["--lines", "2001"], riff::text::daemon_log_bad_lines(2001)),
        (&["--grep", ""], riff::text::daemon_log_bad_grep("")),
    ] {
        let out = daemon_log(&env, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(text(&out.stderr).contains(&says), "{}", text(&out.stderr));
    }
}

/// #2156: the book has the how-to of `riff daemon log`, also from the
/// lead with `--host`. No step of the book reads the journal with `grep`
/// now. Each step of `riff daemon log` in the book runs: with no
/// `--host`, on a log with a line for each text of its `--grep`, which it
/// shows. With `--host`, its flags are real.
#[test]
fn the_book_shows_how_to_read_the_log_of_the_daemon_from_the_lead() {
    let page = book::page("development.md");
    for heading in [
        "### Read the log of the daemon",
        "### Read the log of the daemon from the lead",
        "### See more lines of the log of the daemon",
    ] {
        assert!(page.lines().any(|l| l == heading), "{heading}");
    }
    let commands = book::riff_commands_of("development.md");
    let want = "riff daemon log --host thelio --since 02:12 --grep issue-12";
    assert!(commands.iter().any(|c| c == want), "{commands:?}");

    let mut steps = Vec::new();
    for name in book::pages() {
        for command in book::commands_of(&name) {
            let journal = command.starts_with("journalctl") && command.contains("grep");
            assert!(!journal, "{name}: {command}");
            if command.starts_with("riff daemon log") {
                steps.push(book::words(&command));
            }
        }
    }
    assert!(steps.len() >= 10, "{steps:?}");
    let env = Isolated::new();
    // A line after now: each --since of the book picks it.
    let at = Utc::now() + Duration::minutes(1);
    for words in steps {
        let args: Vec<&str> = words[3..].iter().map(String::as_str).collect();
        if args.contains(&"--host") {
            let out = env.riff().args(&words[1..]).arg("--help").output().unwrap();
            assert!(out.status.success(), "{words:?}: {}", text(&out.stderr));
            continue;
        }
        let texts = args
            .windows(2)
            .filter(|pair| pair[0] == "--grep")
            .map(|pair| pair[1]);
        let mut lines: Vec<String> = texts
            .map(|t| line(at, &format!("riff daemon: {t}")))
            .collect();
        if lines.is_empty() {
            lines.push(line(at, "riff daemon: a line"));
        }
        write_log(&env, &[], &lines);
        let out = daemon_log(&env, &args);
        assert!(out.status.success(), "{words:?}: {}", text(&out.stderr));
        let shown = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
        assert_eq!(text(&out.stdout), shown, "{words:?}");
    }
}
