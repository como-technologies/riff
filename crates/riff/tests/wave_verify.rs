//! The look of the wave reads what the thread says of the verify of a
//! head (01M77JWXEGDDMB7TW97APGKW9W, #2014). riff sets the status
//! `riff/verify` before it posts the result in the thread, so the look can
//! read the old failure of a head while the thread already has its PASS.
//! The fake `gh` gives the old failure: the PASS in the thread counts, and
//! the wave starts no build and no verify of that head.

use std::collections::BTreeSet;
use std::path::Path;

use riff::pr::{Gh, Reported, Verdict};
use riff::wave::{self, Work};

const REPO: &str = "o/r";

/// A fake `gh`: the milestones, the file `pulls` for `pr list`, the file
/// `issues` for `issue list`, and a passed Gate for each commit.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
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

/// The issues 1 and 2 of the wave.
const ISSUES: &str = r#"[
{"number":1,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// The head of the pull request #41 of issue 1.
const HEAD: &str = "aaaa1112223334445556";

/// The fake `gh` in `dir`, with the pull request #41 of issue 1 at `head`
/// and the status `riff/verify` with `state` (or none). The rules require
/// `riff/verify`, so GitHub blocks the merge until it is a success.
fn forge(dir: &Path, head: &str, state: Option<&str>) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), ISSUES).unwrap();
    let checks = match state {
        Some(state) => format!(r#"[{{"context":"riff/verify","state":"{state}"}}]"#),
        None => "[]".into(),
    };
    let merge = match state {
        Some("SUCCESS") => "CLEAN",
        _ => "BLOCKED",
    };
    let pull = format!(
        r#"[{{"number":41,"headRefName":"worktree-issue-1","headRefOid":"{head}","isDraft":false,"statusCheckRollup":{checks},"mergeable":"MERGEABLE","mergeStateStatus":"{merge}","autoMergeRequest":{{"enabledAt":"2026-11-12T08:00:00Z","mergeMethod":"SQUASH"}},"body":"Closes #1\n\nIssue: #1\nMilestone: Wave 1\n"}}]"#
    );
    std::fs::write(dir.join("pulls"), pull).unwrap();
    Gh::at(path)
}

/// The verify request of the daemon for the head `head` of issue 1.
fn request(head: &str) -> String {
    riff::text::wave_verify_request("issue-1", 41, "worktree-issue-1", head, "65f510d7")
}

/// The result of a verify of the head `head` of issue 1 in the thread.
fn result(verdict: Verdict, head: &str) -> String {
    let done = Reported {
        issue: 1,
        commit: head.into(),
        url: "https://github.com/o/r/pull/41#issuecomment-7".into(),
    };
    riff::text::verify_post(verdict, 41, &done, "1. it works.\n")
}

/// The free work of the look of the wave from `gh`, with the thread
/// `bodies`, oldest first, and no claim and no hold.
fn free(gh: &Gh, bodies: &[&String]) -> Vec<String> {
    let bodies: Vec<&str> = bodies.iter().map(|b| b.as_str()).collect();
    let look = wave::look_with(gh, REPO, BTreeSet::new(), BTreeSet::new(), &bodies).unwrap();
    look.free
}

/// The head failed its first verify, and a new request asked for a
/// second one. The second verify passed, and the look still reads the old
/// failure: the item has no build and no verify, and issue 2 gets the
/// pick.
#[test]
fn a_pass_after_a_failed_verify_of_the_head_is_no_work_while_the_look_reads_the_old_failure() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), HEAD, Some("FAILURE"));
    let (asked, failed) = (request(HEAD), result(Verdict::Fail, HEAD));
    let passed = result(Verdict::Pass, HEAD);

    // The fail: a build of the item.
    assert_eq!(free(&gh, &[&asked, &failed]), ["issue-1", "issue-2"]);
    // The new request: a verify of the same head.
    assert_eq!(
        free(&gh, &[&asked, &failed, &asked]),
        ["verify-issue-1", "issue-2"]
    );
    // The PASS: no build and no verify, also with the old failure.
    let thread = [&asked, &failed, &asked, &passed];
    assert_eq!(free(&gh, &thread), ["issue-2"]);
    let look = wave::look_with(
        &gh,
        REPO,
        BTreeSet::new(),
        BTreeSet::new(),
        &thread.map(|b| b.as_str()),
    )
    .unwrap();
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &Default::default()),
        Some(Work::Build { issue: 2 })
    );
    assert!(!look.pulls.contains_key(&1), "no verify waits for #41");
    // The old failure explains the block of GitHub: the lead hears nothing.
    assert!(look.fixes.notices.is_empty(), "{:?}", look.fixes.notices);
}

/// A PASS counts only for its own head: a new head with the old failure
/// is a build. With no status yet, the last result of the thread gives
/// the state of the head.
#[test]
fn a_result_in_the_thread_counts_only_for_its_head_and_over_a_missing_status() {
    let dir = tempfile::tempdir().unwrap();
    let (asked, failed) = (request(HEAD), result(Verdict::Fail, HEAD));
    let passed = result(Verdict::Pass, HEAD);

    // The thread has the PASS of another head.
    let other = forge(dir.path(), "bbbb2223334445556667", Some("FAILURE"));
    assert_eq!(
        free(&other, &[&asked, &failed, &asked, &passed]),
        ["issue-1", "issue-2"]
    );

    // The look reads no status yet.
    let none = forge(dir.path(), HEAD, None);
    assert_eq!(free(&none, &[&asked]), ["verify-issue-1", "issue-2"]);
    assert_eq!(free(&none, &[&asked, &failed]), ["issue-1", "issue-2"]);
    assert_eq!(free(&none, &[&asked, &passed]), ["issue-2"]);

    // A success counts over a FAIL in the thread.
    let success = forge(dir.path(), HEAD, Some("SUCCESS"));
    assert_eq!(free(&success, &[&asked, &failed]), ["issue-2"]);
}
