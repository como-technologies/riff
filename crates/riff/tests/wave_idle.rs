//! The look of the wave asks GitHub first whether anything changed
//! (#1566, 01M6JFJQD4H4GG0H0TFSW5G1PD): two conditional reads of REST, each
//! with the `ETag` of its last answer. A fake `gh` answers `304` as GitHub
//! does. When both answer `304` and no pull request is open, the look
//! makes no other call: no GraphQL, no list. A change, or an open pull
//! request, gets the full read.

use std::collections::BTreeSet;
use std::path::Path;

use riff::pr::Gh;
use riff::wave::{self, Look};

const REPO: &str = "o/r";

/// A fake `gh` that logs each call in `calls`. A conditional read
/// (`api -i`) answers `304` when it sends the `ETag` in the file
/// `etag-issues` or `etag-pulls`, else `200` with that `ETag`. The other
/// reads come from the files `pulls`, `issues` and `queue`.
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
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) echo '[]' ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      */check-runs*) echo '{"check_runs":[{"status":"completed","conclusion":"success","head_sha":"x"}]}' ;;
      *) exit 1 ;;
    esac ;;
  "pr list") cat "$dir/pulls" ;;
  "issue list") cat "$dir/issues" ;;
  *) exit 1 ;;
esac
"#;

/// The issues 1 to `last` of the wave.
fn issues(last: u64) -> String {
    let issue = |n: u64| {
        format!(
            r#"{{"number":{n},"body":"Done when: x.","comments":[],"milestone":{{"title":"Wave 1"}},"labels":[]}}"#
        )
    };
    let all: Vec<String> = (1..=last).map(issue).collect();
    format!("[{}]", all.join(","))
}

/// An open pull request of issue 1, with a passed verify.
const PULL: &str = r#"{"number":41,"headRefName":"worktree-issue-1","headRefOid":"abc1234","isDraft":false,"statusCheckRollup":[{"context":"riff/verify","state":"SUCCESS"}],"mergeable":"MERGEABLE","autoMergeRequest":null,"body":"Closes #1\n\nIssue: #1\nMilestone: Wave 1\n"}"#;

/// The fake `gh` in `dir`, with the open pull requests `pulls` and the
/// issues 1 to 3.
fn forge(dir: &Path, pulls: &[&str]) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    let put = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    put("issues", &issues(3));
    put("pulls", &format!("[{}]", pulls.join(",")));
    put(
        "queue",
        r#"{"data":{"repository":{"pullRequests":{"nodes":[]}}}}"#,
    );
    put("etag-issues", r#"W/"issues-1""#);
    put("etag-pulls", r#"W/"pulls-1""#);
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and no hold.
fn look(gh: &Gh) -> Look {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap()
}

/// The calls of the fake `gh` since the last time this function ran.
fn calls(dir: &Path) -> Vec<String> {
    let file = dir.join("calls");
    let calls = std::fs::read_to_string(&file).unwrap_or_default();
    let _ = std::fs::remove_file(&file);
    calls.lines().map(str::to_owned).collect()
}

/// True when the call is a conditional read of the list `list` with an
/// `ETag`.
fn conditional(call: &str, list: &str) -> bool {
    call.starts_with("api -i -H If-None-Match: ")
        && call.ends_with(&format!(
            "repos/o/r/{list}?state=all&sort=updated&direction=desc&per_page=1"
        ))
}

#[test]
fn an_idle_look_makes_only_the_two_conditional_calls_and_no_graphql_call() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[]);

    // The first look has no `ETag` to send: the full read, with the
    // conditional reads that give the `ETag`s.
    let first = look(&gh);
    assert_eq!(first.free, ["issue-1", "issue-2", "issue-3"]);
    let calls_first = calls(dir.path());
    assert!(
        calls_first.iter().any(|c| c.starts_with("pr list")),
        "{calls_first:?}"
    );
    assert!(
        calls_first.iter().any(|c| c.starts_with("issue list")),
        "{calls_first:?}"
    );
    assert!(
        calls_first.iter().any(|c| c.starts_with("api graphql")),
        "{calls_first:?}"
    );

    // Nothing changed: the same look from two calls, both conditional.
    let second = look(&gh);
    assert_eq!(second, first);
    let calls_second = calls(dir.path());
    assert_eq!(calls_second.len(), 2, "{calls_second:?}");
    assert!(conditional(&calls_second[0], "issues"), "{calls_second:?}");
    assert!(conditional(&calls_second[1], "pulls"), "{calls_second:?}");
    assert!(
        calls_second[0].contains(r#"W/"issues-1""#),
        "{calls_second:?}"
    );
    assert!(
        calls_second[1].contains(r#"W/"pulls-1""#),
        "{calls_second:?}"
    );

    // And so at each next look.
    assert_eq!(look(&gh), first);
    assert_eq!(calls(dir.path()).len(), 2);
}

#[test]
fn a_change_that_github_reports_gets_the_full_read_and_the_new_etag() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[]);
    let first = look(&gh);
    calls(dir.path());

    // An issue closes: the newest issue changes, and so does its `ETag`.
    std::fs::write(dir.path().join("issues"), issues(2)).unwrap();
    std::fs::write(dir.path().join("etag-issues"), r#"W/"issues-2""#).unwrap();
    let changed = look(&gh);
    assert_eq!(changed.free, ["issue-1", "issue-2"]);
    assert_ne!(changed, first);
    let calls_changed = calls(dir.path());
    assert!(
        calls_changed.iter().any(|c| c.starts_with("issue list")),
        "{calls_changed:?}"
    );

    // The look keeps the new `ETag`: the next look is idle again.
    assert_eq!(look(&gh), changed);
    let calls_idle = calls(dir.path());
    assert_eq!(calls_idle.len(), 2, "{calls_idle:?}");
    assert!(calls_idle[0].contains(r#"W/"issues-2""#), "{calls_idle:?}");
}

#[test]
fn a_change_of_the_pull_requests_alone_gets_the_full_read() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[]);
    look(&gh);
    calls(dir.path());

    // A pull request opens: only the list of pull requests answers `200`.
    std::fs::write(dir.path().join("pulls"), format!("[{PULL}]")).unwrap();
    std::fs::write(dir.path().join("etag-pulls"), r#"W/"pulls-2""#).unwrap();
    look(&gh);
    let calls_changed = calls(dir.path());
    assert!(
        calls_changed.iter().any(|c| c.starts_with("pr list")),
        "{calls_changed:?}"
    );
}

#[test]
fn an_open_pull_request_gets_the_full_read_at_each_look() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[PULL]);
    // The checks and the `mergeable` state of a pull request change no
    // `updated_at`: GitHub answers `304`, and the look reads in full.
    for _ in 0..3 {
        look(&gh);
        let calls = calls(dir.path());
        assert!(calls.iter().any(|c| c.starts_with("pr list")), "{calls:?}");
        assert!(
            calls.iter().any(|c| c.starts_with("issue list")),
            "{calls:?}"
        );
    }
}

#[test]
fn a_gh_that_cannot_answer_the_conditional_read_gets_the_full_read() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[]);
    // A `gh` that fails the conditional reads, or gives no `ETag`.
    let script = FAKE_GH.replace(r#""api -i")"#, r#""api -x")"#);
    isolated::write_program(dir.path().join("gh"), &script);
    for _ in 0..2 {
        look(&gh);
        let calls = calls(dir.path());
        assert!(calls.iter().any(|c| c.starts_with("pr list")), "{calls:?}");
    }
}
