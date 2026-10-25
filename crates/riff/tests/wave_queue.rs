//! The wave of the daemon and the merge queue of the default branch
//! (01M5TKPNDET56VR8WZ74FSR71F, 01M5TKPNGG27Z1SQKTA296D9H7): the look of
//! the wave reads the queue with one `gh api graphql` call from a fake
//! `gh`. A pull request in the queue gets no rebase and no message. A
//! pull request that the queue removed gets a rebase when it has a
//! conflict, else one message for the lead. A `gh` that cannot read the
//! queue leaves the wave as it was.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use riff::pr::Gh;
use riff::rollout::{Queue, Stuck};
use riff::wave::{self, Conflict, Work};

const REPO: &str = "o/r";

/// A fake `gh`: the file `pulls` for `pr list`, the file `issues` for
/// `issue list`, and the file `queue` for the GraphQL call of the queue.
/// With no file `queue`, that call fails.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/calls"
case "$1 $2" in
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

/// The issues 1 to 4 of the wave.
const ISSUES: &str = r#"[
{"number":1,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":2,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":3,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]},
{"number":4,"body":"Done when: x.","comments":[],"milestone":{"title":"Wave 1"},"labels":[]}]"#;

/// The pull request of `issue` at `head`, with a passed verify, the state
/// `mergeable`, and auto-merge on when `auto` is true.
fn pull(issue: u64, head: &str, mergeable: &str, auto: bool) -> String {
    let auto = match auto {
        true => r#"{"enabledAt":"2026-10-25T08:00:00Z","mergeMethod":"SQUASH"}"#,
        false => "null",
    };
    format!(
        r#"{{"number":{},"headRefName":"worktree-issue-{issue}","headRefOid":"{head}","isDraft":false,"statusCheckRollup":[{{"context":"riff/verify","state":"SUCCESS"}}],"mergeable":"{mergeable}","autoMergeRequest":{auto},"body":"Closes #{issue}\n\nIssue: #{issue}\nMilestone: Wave 1\n"}}"#,
        40 + issue
    )
}

/// The node of the reply of the queue for the pull request `number`: in
/// the queue or not, and the last event of its timeline, if any.
fn node(number: u64, in_queue: bool, last: Option<&str>) -> String {
    let nodes = last
        .map(|event| format!("[{event}]"))
        .unwrap_or("[]".into());
    format!(
        r#"{{"number":{number},"isInMergeQueue":{in_queue},"timelineItems":{{"nodes":{nodes}}}}}"#
    )
}

const ADDED: &str = r#"{"__typename":"AddedToMergeQueueEvent"}"#;
const PUSHED: &str = r#"{"__typename":"PullRequestCommit"}"#;

fn removed(reason: &str) -> String {
    format!(r#"{{"__typename":"RemovedFromMergeQueueEvent","reason":"{reason}"}}"#)
}

/// The fake `gh` in `dir`, with the open pull requests `pulls`, and the
/// reply `nodes` of the queue. With `None` the call of the queue fails.
fn forge(dir: &Path, pulls: &[String], nodes: Option<&[String]>) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    std::fs::write(dir.join("issues"), ISSUES).unwrap();
    std::fs::write(dir.join("pulls"), format!("[{}]", pulls.join(","))).unwrap();
    match nodes {
        Some(nodes) => std::fs::write(
            dir.join("queue"),
            format!(
                r#"{{"data":{{"repository":{{"pullRequests":{{"nodes":[{}]}}}}}}}}"#,
                nodes.join(",")
            ),
        )
        .unwrap(),
        None => drop(std::fs::remove_file(dir.join("queue"))),
    }
    Gh::at(path)
}

/// The look of the wave from `gh`, with no claim and no hold.
fn look(gh: &Gh) -> wave::Look {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap()
}

/// The pull requests 41 and 42 have a conflict with `main`, and 43 has
/// auto-merge off. All three are in the queue: the look has no rebase and
/// no notice for them, and the free work is the item 4 only. Without the
/// queue, the same pull requests get two rebases and a notice.
#[test]
fn a_pull_request_in_the_queue_gets_no_rebase_and_no_notice() {
    let dir = tempfile::tempdir().unwrap();
    let pulls = [
        pull(1, "aaaa111", "CONFLICTING", true),
        pull(2, "bbbb222", "CONFLICTING", true),
        pull(3, "cccc333", "MERGEABLE", false),
    ];
    let gh = forge(
        dir.path(),
        &pulls,
        Some(&[
            node(41, true, Some(ADDED)),
            node(42, true, Some(ADDED)),
            node(43, true, Some(ADDED)),
        ]),
    );
    let inside = look(&gh);
    assert_eq!(inside.free, ["issue-4"]);
    assert!(inside.fixes.conflicts.is_empty(), "{:?}", inside.fixes);
    assert!(inside.fixes.notices.is_empty(), "{:?}", inside.fixes);
    assert_eq!(
        wave::pick(&inside, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Build { issue: 4 })
    );

    // The same pull requests outside the queue.
    let gh = forge(dir.path(), &pulls, Some(&[]));
    let outside = look(&gh);
    assert_eq!(
        outside.fixes.conflicts.keys().copied().collect::<Vec<_>>(),
        [1, 2]
    );
    let told: Vec<_> = outside
        .fixes
        .notices
        .iter()
        .map(|n| n.stuck.clone())
        .collect();
    assert_eq!(told, [Stuck::AutoMergeOff]);
}

/// The queue removed 41 with a conflict, and auto-merge is off: a rebase.
/// It removed 42 with no conflict: one notice for the lead with the
/// reason of GitHub, and no session. A push to 43 came after its removal:
/// the removal is old, and nothing is stuck.
#[test]
fn a_pull_request_that_the_queue_removed_gets_a_rebase_or_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", "CONFLICTING", false),
            pull(2, "bbbb222", "MERGEABLE", true),
            pull(3, "cccc333", "MERGEABLE", true),
        ],
        Some(&[
            node(41, false, Some(&removed("MERGE_CONFLICT"))),
            node(42, false, Some(&removed("CI_FAILURE"))),
            node(43, false, Some(PUSHED)),
        ]),
    );
    let look = look(&gh);
    assert_eq!(look.free, ["issue-1", "issue-4"]);
    assert_eq!(
        look.fixes.conflicts,
        BTreeMap::from([(
            1,
            Conflict {
                pull: 41,
                commit: "aaaa111".into()
            }
        )])
    );
    assert_eq!(
        wave::pick(&look, &BTreeSet::new(), &BTreeMap::new()),
        Some(Work::Rebase {
            issue: 1,
            pull: 41,
            commit: "aaaa111".into()
        })
    );
    assert_eq!(look.fixes.notices.len(), 1, "{:?}", look.fixes.notices);
    let notice = &look.fixes.notices[0];
    assert_eq!(notice.pull, 42);
    assert_eq!(notice.stuck, Stuck::Dequeued("CI_FAILURE".into()));
    assert_eq!(notice.key(), "#42@bbbb222:removed from queue");
}

/// One call reads the queue for all pull requests, and it names the
/// repository and no more.
#[test]
fn the_look_reads_the_queue_with_one_graphql_call() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(
        dir.path(),
        &[
            pull(1, "aaaa111", "MERGEABLE", true),
            pull(2, "bbbb222", "MERGEABLE", true),
        ],
        Some(&[node(41, true, Some(ADDED))]),
    );
    look(&gh);
    let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
    let graphql: Vec<&str> = calls
        .lines()
        .filter(|c| c.starts_with("api graphql"))
        .collect();
    assert_eq!(graphql.len(), 1, "{calls}");
    assert!(
        graphql[0].contains("-f owner=o -f name=r"),
        "{}",
        graphql[0]
    );
    assert!(graphql[0].contains("isInMergeQueue"), "{}", graphql[0]);
    assert!(
        graphql[0].contains("REMOVED_FROM_MERGE_QUEUE_EVENT"),
        "{}",
        graphql[0]
    );
}

/// A `gh` that cannot read the queue, and a repository with no queue,
/// leave the wave as it was: a conflict with auto-merge on is a rebase.
#[test]
fn a_fault_of_the_queue_call_leaves_the_wave_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path(), &[pull(1, "aaaa111", "CONFLICTING", true)], None);
    let look = look(&gh);
    assert_eq!(look.free[0], "issue-1");
    assert!(look.fixes.conflicts.contains_key(&1));
    let queue = riff::rollout::queue_of(&gh, REPO);
    assert!(queue.is_empty());
    assert_eq!(queue.get(&41).cloned().unwrap_or_default(), Queue::Out);
}
