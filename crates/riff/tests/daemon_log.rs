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
    env.riff()
        .args(["daemon", "log"])
        .args(args)
        .env("TZ", "UTC")
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

    // No line matches.
    let out = daemon_log(&env, &["--grep", "issue-99"]);
    assert!(out.status.success());
    assert_eq!(
        text(&out.stdout),
        "riff daemon log: no line of the log of the riff daemon matches.\n"
    );
}

/// #2156, 01M7EX1Q1F468NWGPJJS0W0ZG9: a line with a token-like text
/// shows with `[secret]` in its place, and `--grep` finds no part of a
/// secret: a token of GitHub, a key of Anthropic, `NAME=value` of a
/// token, a bearer token, and the value of a secret variable of the host.
#[test]
fn a_line_with_a_token_like_text_is_filtered() {
    let env = Isolated::new();
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
    ];
    write_log(&env, &[], &lines);
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
             {stamp} riff daemon: the token [secret] of the host\n"
        )
    );
    for probe in [
        "ghs_16C7",
        "sk-ant-oat01",
        "GH_TOKEN=abc",
        "Bearer xyz",
        "a-token-of",
    ] {
        let out = daemon_log(&env, &["--grep", probe]);
        assert_eq!(
            text(&out.stdout),
            "riff daemon log: no line of the log of the riff daemon matches.\n",
            "{probe}"
        );
    }
}

/// #2156, 01M7EX1PVSG64G8EE5SNDRHM7G: with no log file, riff names the
/// file and the command that shows the log, and exits with 1. A bad flag
/// says what it takes.
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
    for (args, says) in [
        (
            &["--since", "noon"][..],
            riff::text::daemon_log_bad_since("noon"),
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
/// lead with `--host`, and each command of it runs.
#[test]
fn the_book_shows_how_to_read_the_log_of_the_daemon_from_the_lead() {
    let page = book::page("development.md");
    for heading in [
        "### Read the log of the daemon",
        "### Read the log of the daemon from the lead",
    ] {
        assert!(page.lines().any(|l| l == heading), "{heading}");
    }
    let commands = book::riff_commands_of("development.md");
    let want = "riff daemon log --host thelio --since 02:12 --grep issue-12";
    assert!(commands.iter().any(|c| c == want), "{commands:?}");
    // A text in quotes is more than one word: those commands stay out.
    let ours: Vec<String> = commands
        .into_iter()
        .filter(|c| c.starts_with("riff daemon log") && !c.contains(['\'', '"']))
        .collect();
    assert!(ours.len() >= 2, "{ours:?}");
    book::each_is_real(&ours);
}
