//! The points that riff counts for the calls of `gh`, and the line of
//! `riff daemon status` for the callers that riff does not see (#1607).
//!
//! The first test runs a fake `gh` and shows the line with the points that
//! riff counted and the points that the counter used, both from the time of
//! the reading. The second test is ignored: it measures the cost of the
//! queries against GitHub again, with `rateLimit(dryRun: true)`, and
//! compares it with `src/ghcalls/costs.txt`. It needs a `gh` that is signed
//! in, and it makes a few real calls of one point each:
//!
//! ```sh
//! cargo test -p riff --test all gh_costs:: -- --ignored --nocapture
//! ```
//!
//! `RIFF_COST_REPO` names the repository for the queries (the default is
//! `como-technologies/riff`).

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use riff::ghcalls::Calls;
use riff::pr::Gh;

/// A fake `gh`: `pr list` answers with an empty list. A query of GraphQL
/// answers with a repository and a `rateLimit` that shows `remaining`
/// points and the reset `reset_at`.
fn fake_gh(dir: &Path, remaining: u64, reset_at: &str) -> Gh {
    let script = format!(
        r#"#!/bin/sh
case "$*" in
  "api graphql"*) echo '{{"data":{{"rateLimit":{{"cost":1,"limit":5000,"remaining":{remaining},"resetAt":"{reset_at}"}},"repository":{{}}}}}}' ;;
  "pr list"*) echo '[]' ;;
esac
"#
    );
    Gh::at(isolated::write_program(dir.join("gh"), &script))
}

/// The reset of a counter that started `started_ago` ms ago, and the
/// time as `gh` says it.
fn counter_reset(started_ago: i64) -> (i64, String) {
    let reset_ms = riff::daemon::now_ms() - started_ago + 3_600_000;
    let at = chrono::DateTime::from_timestamp_millis(reset_ms)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    (reset_ms, at)
}

/// A full look of the wave: 100 pull requests with their checks, 2 points.
const LOOK: [&str; 8] = [
    "pr",
    "list",
    "--state",
    "open",
    "--limit",
    "100",
    "--json",
    "number,statusCheckRollup",
];

/// 01M6QBRR7Q2NXCARHE21572GQ5 (#1607): the line of the other callers
/// shows the points that riff counted and the points that the counter used
/// at the time of the reading, and a call after the reading is not in
/// them. The line says that time.
#[test]
fn the_other_line_shows_the_counted_points_and_the_counter_used_from_the_reading() {
    let dir = tempfile::tempdir().unwrap();
    // A counter that started 20 minutes ago: it used 400 points.
    let (reset_ms, reset_at) = counter_reset(20 * 60_000);
    let calls = Arc::new(Calls::default());
    let gh = fake_gh(dir.path(), 5_000 - 400, &reset_at).with_calls(calls.clone());
    for _ in 0..10 {
        gh.run(&LOOK, None).unwrap();
    }
    // The reading: a query of GraphQL that asks for `rateLimit`. It
    // costs 1 point and shows the counter.
    gh.run(&["api", "graphql", "-f", "query=x"], None).unwrap();
    let shown = calls.shown(riff::daemon::now_ms());
    let read_ms = shown.left.as_ref().expect("a reading").read_ms;
    // A call after the reading: the next status counts it in its rows,
    // but not in the comparison with this reading.
    std::thread::sleep(std::time::Duration::from_millis(5));
    gh.run(&LOOK, None).unwrap();

    let shown = calls.shown(riff::daemon::now_ms());
    assert_eq!(shown.total(), (12, 10 * 2 + 1 + 2));
    let other = shown.other.expect("the other line");
    assert_eq!((other.used, other.counted, other.points), (400, 21, 379));
    assert_eq!(other.read_ms, read_ms);
    // The reset has whole seconds.
    assert_eq!(other.since_ms, reset_ms / 1000 * 1000 - 3_600_000);

    let text = riff::text::github_use(&shown, riff::daemon::now_ms());
    let line = text
        .lines()
        .find(|line| line.starts_with("Other callers: "))
        .expect("the other line");
    assert!(
        line.starts_with("Other callers: 379 GraphQL points since "),
        "{line}"
    );
    let at = riff::text::clock_utc(read_ms);
    assert!(
        line.ends_with(&format!(
            "At {at} the counter used 400, and riff counted 21."
        )),
        "{line}"
    );
}

/// 01M6QBRR7Q2NXCARHE21572GQ5 (#1607): when riff counts more points than
/// the counter used, the line says so in place of 0.
#[test]
fn the_other_line_says_when_riff_counted_more_than_the_counter_used() {
    let dir = tempfile::tempdir().unwrap();
    // The counter used 332 points. The calls of riff count 1,201 of them,
    // as when the calls land on more than one counter of the token.
    let (_, reset_at) = counter_reset(8 * 60_000);
    let calls = Arc::new(Calls::default());
    let gh = fake_gh(dir.path(), 5_000 - 332, &reset_at).with_calls(calls.clone());
    for _ in 0..600 {
        gh.run(&LOOK, None).unwrap();
    }
    gh.run(&["api", "graphql", "-f", "query=x"], None).unwrap();

    let shown = calls.shown(riff::daemon::now_ms());
    let other = shown.other.expect("the other line");
    assert_eq!((other.used, other.counted, other.points), (332, 1_201, 0));
    assert!(other.counted_more());

    let text = riff::text::github_use(&shown, riff::daemon::now_ms());
    let at = riff::text::clock_utc(other.read_ms);
    let line = format!(
        "Other callers: not known. riff counted more than GitHub used: the points of riff are \
         estimates. At {at} the counter used 332, and riff counted 1,201.\n"
    );
    assert!(text.ends_with(&line), "{text}");
    assert!(!text.contains("GraphQL points since"), "{text}");
}

/// The rows of `src/ghcalls/costs.txt`: the cost that GitHub gave, the
/// items of the page, and the arguments of `gh`.
fn recorded() -> Vec<(u32, usize, Vec<&'static str>)> {
    include_str!("../src/ghcalls/costs.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut parts = line.splitn(3, " | ");
            let cost = parts.next().unwrap().parse().unwrap();
            let first = parts.next().unwrap().parse().unwrap();
            (cost, first, parts.next().unwrap().split(' ').collect())
        })
        .collect()
}

/// 01M6QBRR4GDSNZ9YPTDR3AJ0KA (#1607): the points that riff counts for a
/// call are the points that GitHub gave for its query in a dry run, in the
/// record of the measurement.
#[test]
fn the_points_of_a_list_are_the_cost_that_github_gave() {
    let rows = recorded();
    assert!(rows.len() >= 30, "{} rows", rows.len());
    for (cost, first, args) in rows {
        assert_eq!(
            riff::ghcalls::points_of(&args, "[]"),
            cost,
            "gh {} (a page of {first})",
            args.join(" ")
        );
    }
}

/// The query that `gh ARGS` sends, from `GH_DEBUG=api`, with a read of
/// one item (a call of one point).
fn query_of(repo: &str, args: &[&str]) -> String {
    let mut once: Vec<&str> = Vec::new();
    let mut skip = false;
    for arg in args {
        match (skip, *arg) {
            (true, _) => skip = false,
            (false, "--limit") => skip = true,
            (false, other) => once.push(other),
        }
    }
    let out = Command::new("gh")
        .args(&once)
        .args(["--repo", repo, "--limit", "1"])
        .env("GH_DEBUG", "api")
        .output()
        .expect("gh");
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    let (_, query) = log
        .split_once("GraphQL query:\n")
        .expect("a query in the log");
    let (query, _) = query.split_once("\nGraphQL variables:").expect("variables");
    query.to_owned()
}

/// The `cost` that GitHub gives for `query` with `first` items, in a dry
/// run.
fn dry_run(repo: &str, query: &str, first: usize) -> Option<u32> {
    let (owner, name) = repo.split_once('/').expect("OWNER/REPO");
    let query = query.replacen(
        "repository(owner: $owner, name: $repo) {",
        "rateLimit(dryRun: true) { cost } repository(owner: $owner, name: $repo) {",
        1,
    );
    let mut cmd = Command::new("gh");
    cmd.args(["api", "graphql", "-f", &format!("query={query}")])
        .args(["-F", &format!("limit={first}")])
        .args([
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("repo={name}"),
        ]);
    if query.contains("$states") {
        cmd.args(["-f", "states[]=OPEN"]);
    }
    if query.contains("$state:") {
        cmd.args(["-f", "state[]=OPEN"]);
    }
    let out = cmd.output().expect("gh");
    let reply: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    u32::try_from(reply["data"]["rateLimit"]["cost"].as_u64()?).ok()
}

/// Measures every row of the record again. It fails when GitHub changes
/// a cost: then change the record, and `points_of`.
#[test]
#[ignore = "calls GitHub: measure the cost of the queries again"]
fn measure_the_cost_of_a_list() {
    let repo = std::env::var("RIFF_COST_REPO").unwrap_or_else(|_| "como-technologies/riff".into());
    for (cost, first, args) in recorded() {
        let query = query_of(&repo, &args);
        assert_eq!(
            dry_run(&repo, &query, first),
            Some(cost),
            "gh {} (a page of {first})",
            args.join(" ")
        );
    }
}
