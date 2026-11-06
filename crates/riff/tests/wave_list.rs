//! `riff wave list` keeps its read of GitHub on the host for 60 seconds
//! (#1609, 01M6QBEV0PNMAESMW620AF5HH7). A fake `gh` answers `304` as
//! GitHub does, and logs each call.

use std::path::Path;
use std::time::Duration;

use riff::pr::Gh;
use riff::wave_list::{Listed, read_in};

const REPO: &str = "o/r";

/// A fake `gh` that logs each call in `calls`. A conditional read
/// (`api -i`) answers `304` when it sends the `ETag` in the file
/// `etag-issues` or `etag-pulls`, else `200` with that `ETag`. The other
/// reads come from the files `milestones`, `pulls`, `issues` and `queue`.
/// The file `fail` makes `issue list` fail.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$1 $2" in
  "api -i")
    case "$*" in *issues*) tag=$(cat "$dir/etag-issues") ;; *) tag=$(cat "$dir/etag-pulls") ;; esac
    case "$*" in
      *"If-None-Match: $tag"*) echo 'gh: HTTP 304' >&2; exit 1 ;;
      *) printf 'HTTP/2.0 200 OK\r\nEtag: %s\r\n\r\n[]\n' "$tag" ;;
    esac ;;
  "api graphql") cat "$dir/queue" ;;
  api*)
    case "$2" in
      */milestones*) cat "$dir/milestones" ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list")
    if [ -f "$dir/fail" ]; then echo 'GraphQL: API rate limit already exceeded' >&2; exit 1; fi
    cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The open milestones: Wave 1 with `open` open issues, and the Backlog.
fn milestones(open: u64) -> String {
    format!(
        r#"[{{"number":1,"title":"Wave 1","open_issues":{open},"closed_issues":2}},{{"number":2,"title":"Backlog","open_issues":0,"closed_issues":0}}]"#
    )
}

/// The issues 1 to `last` of Wave 1.
fn issues(last: u64) -> String {
    let issue = |n: u64| {
        format!(
            r#"{{"number":{n},"title":"t","body":"Done when: x.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[]}}"#
        )
    };
    let all: Vec<String> = (1..=last).map(issue).collect();
    format!("[{}]", all.join(","))
}

/// An open pull request of issue 1, with a passed verify.
const PULL: &str = r#"{"number":41,"headRefName":"worktree-issue-1","headRefOid":"abc1234","isDraft":false,"statusCheckRollup":[{"context":"riff/verify","state":"SUCCESS"}],"mergeable":"MERGEABLE","autoMergeRequest":null,"body":"Closes #1\n\nIssue: #1\nMilestone: Wave 1\n"}"#;

/// The host: a folder for the kept read, and the fake `gh` with the
/// issues 1 to 3 and the open pull requests `pulls`.
struct Host {
    /// The folder of the fake `gh` and its files.
    forge: tempfile::TempDir,
    /// The folder of the daemon of the host.
    daemon: tempfile::TempDir,
    gh: Gh,
}

impl Host {
    fn new(pulls: &[&str]) -> Host {
        let forge = tempfile::tempdir().unwrap();
        let path = forge.path().join("gh");
        isolated::write_program(&path, FAKE_GH);
        let host = Host {
            forge,
            daemon: tempfile::tempdir().unwrap(),
            gh: Gh::at(path),
        };
        host.put("milestones", &milestones(3));
        host.put("issues", &issues(3));
        host.put("pulls", &format!("[{}]", pulls.join(",")));
        host.put(
            "queue",
            r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#,
        );
        host.put("etag-issues", r#"W/"issues-1""#);
        host.put("etag-pulls", r#"W/"pulls-1""#);
        host
    }

    fn put(&self, name: &str, text: &str) {
        std::fs::write(self.forge.path().join(name), text).unwrap();
    }

    /// A run of `riff wave list` at `secs` seconds.
    fn run(&self, secs: u64, fresh: bool) -> Listed {
        let now_ms = i64::try_from(secs * 1000).unwrap();
        read_in(Some(self.daemon.path()), &self.gh, REPO, fresh, now_ms).unwrap()
    }

    /// The calls of the fake `gh` since the last time this ran.
    fn calls(&self) -> Vec<String> {
        let file = self.forge.path().join("calls");
        let calls = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        calls.lines().map(str::to_owned).collect()
    }
}

/// True when some call starts with `start`.
fn has(calls: &[String], start: &str) -> bool {
    calls.iter().any(|c| c.starts_with(start))
}

/// True when the calls have a full read of the issues and the pull
/// requests.
fn full(calls: &[String]) -> bool {
    has(calls, "issue list") && has(calls, "pr list")
}

fn free_items(listed: &Listed) -> Vec<u64> {
    listed.issues.as_ref().unwrap().waves[0].1.clone()
}

#[test]
fn the_first_run_reads_github_in_full() {
    let host = Host::new(&[]);
    let first = host.run(0, false);
    assert_eq!(free_items(&first), [1, 2, 3]);
    assert_eq!(first.milestones[0].title, "Wave 1");
    assert_eq!(first.age, Duration::ZERO);
    let calls = host.calls();
    assert!(full(&calls), "{calls:?}");
}

#[test]
fn a_run_within_60_seconds_shares_the_read_and_makes_no_call() {
    let host = Host::new(&[]);
    let first = host.run(0, false);
    host.calls();
    // A `watch` each 2 seconds: the same read, and its age.
    for secs in [2, 4, 30, 59] {
        let again = host.run(secs, false);
        assert_eq!(again.milestones, first.milestones);
        assert_eq!(again.issues, first.issues);
        assert_eq!(again.age, Duration::from_secs(secs));
        assert_eq!(host.calls(), Vec::<String>::new(), "at {secs} seconds");
    }
}

#[test]
fn a_run_after_60_seconds_with_no_change_costs_no_graphql_call() {
    let host = Host::new(&[]);
    let first = host.run(0, false);
    host.calls();

    let again = host.run(61, false);
    assert_eq!(again.issues, first.issues);
    // The read is as new as now.
    assert_eq!(again.age, Duration::ZERO);
    let calls = host.calls();
    // The two conditional reads, and the open milestones: all REST.
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(
        calls[0].starts_with("api -i -H If-None-Match: "),
        "{calls:?}"
    );
    assert!(calls[0].contains(r#"W/"issues-1""#), "{calls:?}");
    assert!(calls[1].contains(r#"W/"pulls-1""#), "{calls:?}");
    assert!(!has(&calls, "issue list"), "{calls:?}");
    assert!(!has(&calls, "pr list"), "{calls:?}");
    assert!(!has(&calls, "api graphql"), "{calls:?}");

    // The renewed read is shared for 60 seconds more.
    host.run(100, false);
    assert_eq!(host.calls(), Vec::<String>::new());
}

#[test]
fn a_change_on_github_gets_the_full_read_and_the_new_etag() {
    let host = Host::new(&[]);
    host.run(0, false);
    host.calls();

    // An issue closes: the newest issue changes, and so does its `ETag`.
    host.put("issues", &issues(2));
    host.put("milestones", &milestones(2));
    host.put("etag-issues", r#"W/"issues-2""#);
    let changed = host.run(61, false);
    assert_eq!(free_items(&changed), [1, 2]);
    assert_eq!(changed.milestones[0].open_issues, 2);
    let calls = host.calls();
    assert!(full(&calls), "{calls:?}");

    // The run kept the new `ETag`.
    let idle = host.run(122, false);
    assert_eq!(idle.issues, changed.issues);
    let calls = host.calls();
    assert!(calls[0].contains(r#"W/"issues-2""#), "{calls:?}");
    assert!(!full(&calls), "{calls:?}");
}

#[test]
fn an_open_pull_request_gets_the_full_read_after_60_seconds() {
    let host = Host::new(&[PULL]);
    host.run(0, false);
    host.calls();
    // Within 60 seconds the runs share the read, also with a pull request.
    host.run(30, false);
    assert_eq!(host.calls(), Vec::<String>::new());
    // The checks of a pull request change no `updated_at`: GitHub answers
    // `304`, and the run reads in full once in 60 seconds.
    host.run(61, false);
    assert!(full(&host.calls()));
    host.run(90, false);
    assert_eq!(host.calls(), Vec::<String>::new());
    host.run(122, false);
    assert!(full(&host.calls()));
}

#[test]
fn a_new_or_renamed_milestone_gets_the_full_read() {
    let host = Host::new(&[]);
    host.run(0, false);
    host.calls();
    // A rename changes no issue: both answers are `304`.
    host.put(
        "milestones",
        r#"[{"number":1,"title":"Wave 1: Cloud","open_issues":3,"closed_issues":2}]"#,
    );
    let renamed = host.run(61, false);
    assert_eq!(renamed.milestones[0].title, "Wave 1: Cloud");
    let calls = host.calls();
    assert!(full(&calls), "{calls:?}");
}

#[test]
fn fresh_reads_github_at_once() {
    let host = Host::new(&[]);
    host.run(0, false);
    host.calls();
    host.put("issues", &issues(1));
    host.put("milestones", &milestones(1));
    // No change of the `ETag`, and 2 seconds after the first read.
    let fresh = host.run(2, true);
    assert_eq!(free_items(&fresh), [1]);
    assert_eq!(fresh.age, Duration::ZERO);
    let calls = host.calls();
    assert!(full(&calls), "{calls:?}");
    // The runs that follow share the new read.
    let after = host.run(10, false);
    assert_eq!(free_items(&after), [1]);
    assert_eq!(after.age, Duration::from_secs(8));
    assert_eq!(host.calls(), Vec::<String>::new());
}

#[test]
fn a_full_read_comes_at_least_each_30_minutes() {
    let host = Host::new(&[]);
    host.run(0, false);
    host.calls();
    // A run each minute, and GitHub says "no change" each time.
    for minute in 1..30 {
        host.run(minute * 61, false);
        assert!(!full(&host.calls()), "at minute {minute}");
    }
    // The last full read began 30 minutes ago.
    host.run(31 * 60, false);
    assert!(full(&host.calls()));
}

#[test]
fn a_read_that_fails_is_not_kept() {
    let host = Host::new(&[]);
    host.put("fail", "");
    let failed = host.run(0, false);
    assert_eq!(
        failed.issues.unwrap_err(),
        "gh issue list failed: GraphQL: API rate limit already exceeded"
    );
    // The milestones show all the same.
    assert_eq!(failed.milestones.len(), 2);
    host.calls();
    // The next run tries again, at once.
    std::fs::remove_file(host.forge.path().join("fail")).unwrap();
    let again = host.run(1, false);
    assert_eq!(free_items(&again), [1, 2, 3]);
    assert!(full(&host.calls()));
}

#[test]
fn a_host_with_no_folder_keeps_nothing() {
    let host = Host::new(&[]);
    for secs in [0, 1] {
        read_in(None, &host.gh, REPO, false, secs).unwrap();
        assert!(full(&host.calls()));
    }
}

#[test]
fn a_file_that_riff_cannot_read_is_no_read() {
    let host = Host::new(&[]);
    host.run(0, false);
    let file = riff::wave_list::path(host.daemon.path(), REPO);
    std::fs::write(&file, "{ not json").unwrap();
    host.calls();
    let again = host.run(1, false);
    assert_eq!(free_items(&again), [1, 2, 3]);
    assert!(full(&host.calls()));
    // The full read wrote the file again.
    assert!(
        std::fs::read_to_string(file)
            .unwrap()
            .contains("\"issues\"")
    );
}

#[test]
fn a_clock_that_went_back_is_no_reason_to_keep_a_read() {
    let host = Host::new(&[]);
    host.run(100, false);
    host.calls();
    let again = host.run(50, false);
    assert_eq!(again.age, Duration::ZERO);
    assert!(!host.calls().is_empty());
}

#[test]
fn each_repository_has_its_own_file() {
    let host = Host::new(&[]);
    host.run(0, false);
    let other = read_in(Some(host.daemon.path()), &host.gh, "o/s", false, 1_000).unwrap();
    assert_eq!(other.age, Duration::ZERO);
    assert!(full(&host.calls()));
    assert!(riff::wave_list::path(host.daemon.path(), REPO).exists());
    assert!(riff::wave_list::path(host.daemon.path(), "o/s").exists());
}

#[test]
fn the_file_is_in_the_folder_wave_list_of_the_daemon() {
    let host = Host::new(&[]);
    host.run(0, false);
    assert!(
        Path::new(host.daemon.path())
            .join("wave-list/o~r.json")
            .exists()
    );
}
