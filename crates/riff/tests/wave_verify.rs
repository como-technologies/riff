//! The look of the wave reads what the thread says of the verify of a
//! head (01M77JWXEGDDMB7TW97APGKW9W, #2014). riff sets the status
//! `riff/verify` before it posts the result in the thread, so the look can
//! read the old failure of a head while the thread already has its PASS.
//! The fake `gh` gives the old failure: the PASS in the thread counts, and
//! the wave starts no build and no verify of that head. A PASS counts
//! only from its right sender (#2027). The build of a head whose verify
//! failed is the fix of that head, and its starts count for that head
//! (#2170).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::{Gh, Reported, Verdict};
use riff::wave::{self, Fail, Work};
use riff_core::wire::{Kind, Message};

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

/// The work session of issue 1: it claims `issue-1`. The riff daemon
/// posts the verify request as it.
const AUTHOR: &str = "riff://ann@heron/o/r?session=65f510d7&claim=issue-1#issue-1";

/// The verify session of issue 1: it claims `verify-issue-1`.
const VERIFIER: &str = "riff://ann@heron/o/r?session=v1&claim=verify-issue-1#verify-issue-1";

/// The message `body` of the thread from `from`: the URI of the sender
/// as riff-server gives it, with its claims.
fn said(from: &str, body: &str) -> Message {
    Message {
        seq: 0,
        from: from.parse().unwrap(),
        to: Vec::new(),
        body: body.into(),
        at_ms: 0,
        kind: Kind::Message,
        sig: None,
        payload: None,
    }
}

/// The verify request of the daemon for the head `head` of issue 1.
fn request(head: &str) -> Message {
    let body = riff::text::wave_verify_request("issue-1", 41, "worktree-issue-1", head, "65f510d7");
    said(AUTHOR, &body)
}

/// The result of a verify of the head `head` of issue 1 in the thread,
/// from `from`.
fn result_from(from: &str, verdict: Verdict, head: &str) -> Message {
    let done = Reported {
        issue: 1,
        commit: head.into(),
        url: "https://github.com/o/r/pull/41#issuecomment-7".into(),
    };
    said(
        from,
        &riff::text::verify_post(verdict, 41, &done, "1. it works.\n"),
    )
}

/// The result of a verify of the head `head` of issue 1 from its verify
/// session.
fn result(verdict: Verdict, head: &str) -> Message {
    result_from(VERIFIER, verdict, head)
}

/// The free work of the look of the wave from `gh`, with the thread
/// `thread`, oldest first, and no claim and no hold.
fn free(gh: &Gh, thread: &[&Message]) -> Vec<String> {
    let messages: Vec<Message> = thread.iter().map(|m| (*m).clone()).collect();
    let look = wave::look_with(gh, REPO, BTreeSet::new(), BTreeSet::new(), &messages).unwrap();
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
    // A PASS from the author does not count: the head waits for its verify.
    let forged = result_from(AUTHOR, Verdict::Pass, HEAD);
    assert_eq!(
        free(&gh, &[&asked, &failed, &asked, &forged]),
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
        &thread.map(Message::clone),
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

/// The look of the wave from `gh`, with no thread, no claim and no hold.
fn look(gh: &Gh) -> wave::Look {
    wave::look_with(gh, REPO, BTreeSet::new(), BTreeSet::new(), &[]).unwrap()
}

/// #2170, 01M7FJWPJKBK2V4X73Z45PHE7V: the look of a head whose verify
/// failed gives the fix of that head, and its starts count for that head.
/// A work session opened the pull request, then two fixes pushed a new
/// head each, and the verify of each head failed: the pick gives a third
/// fix, for the third head. Three fixes of the same head: the pick gives
/// no fix of it, and issue 2 gets the pick.
#[test]
fn the_fix_after_a_fail_counts_its_starts_for_each_head() {
    let dir = tempfile::tempdir().unwrap();
    let heads = [HEAD, "bbbb2223334445556667", "cccc3334445556667778"];
    let fix = |head: &str| Work::Fix {
        issue: 1,
        pull: 41,
        commit: head.into(),
    };
    let none = BTreeSet::new();

    // The verify of the first head failed: the fix of that head.
    let first = look(&forge(dir.path(), heads[0], Some("FAILURE")));
    assert_eq!(first.free, ["issue-1", "issue-2"]);
    let fail = |head: &str| Fail {
        pull: 41,
        commit: head.into(),
    };
    assert_eq!(first.fixes.fails, BTreeMap::from([(1, fail(heads[0]))]));
    let build: BTreeMap<String, u32> = [(Work::Build { issue: 1 }.key(), 1)].into();
    assert_eq!(wave::pick(&first, &none, &build), Some(fix(heads[0])));

    // The build and a fix of each of the first two heads started.
    let starts: BTreeMap<String, u32> = [
        Work::Build { issue: 1 }.key(),
        fix(heads[0]).key(),
        fix(heads[1]).key(),
    ]
    .into_iter()
    .map(|key| (key, 1))
    .collect();
    let third = look(&forge(dir.path(), heads[2], Some("FAILURE")));
    assert_eq!(third.fixes.fails, BTreeMap::from([(1, fail(heads[2]))]));
    assert_eq!(wave::pick(&third, &none, &starts), Some(fix(heads[2])));

    // Three fixes of the third head: no more fix of it.
    let spent: BTreeMap<String, u32> = [(fix(heads[2]).key(), wave::STARTS)].into();
    assert_eq!(
        wave::pick(&third, &none, &spent),
        Some(Work::Build { issue: 2 })
    );

    // The head waits for its verify: no fix.
    let waits = look(&forge(dir.path(), heads[2], None));
    assert!(waits.fixes.fails.is_empty(), "{:?}", waits.fixes.fails);
}
