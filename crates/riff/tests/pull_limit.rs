//! The read of the open pull requests stays under the limit of GitHub
//! (01M5QDWWSYK0J3N24GF8J123JQ, #1355).
//!
//! `gh pr list --limit 100 --json commits` asks for 100 commits of each
//! pull request and 100 authors of each commit. GitHub counts
//! 1,000,000 possible nodes and refuses the whole query. A fake `gh`
//! here refuses such a query as GitHub does, so a field that `gh`
//! expands to a nested connection fails these tests. The WIP check
//! reads the head commit of a pull request that waits for its verify,
//! and no other.

use std::path::Path;

use riff::pr::Gh;
use riff::rollout::{CONNECTIONS, in_verify, pulls};
use riff::top::issues_from_gh;

/// The open pull requests of the fake `gh`, for the branches of
/// issues 12 to 16:
/// - 12 waits for a verify, and its head `aaa` is a WIP commit.
/// - 13 waits for a verify, and its head `bbb` is no WIP commit.
/// - 14 has a passed verify.
/// - 15 is a draft.
/// - 16 waits for a verify, and `gh` fails to read its head `fault`.
const PULLS: &str = r#"[
{"number":40,"headRefName":"worktree-issue-12","headRefOid":"aaa","isDraft":false,"statusCheckRollup":[]},
{"number":41,"headRefName":"worktree-issue-13","headRefOid":"bbb","isDraft":false,"statusCheckRollup":[]},
{"number":42,"headRefName":"worktree-issue-14","headRefOid":"ccc","isDraft":false,
 "statusCheckRollup":[{"context":"riff/verify","state":"SUCCESS"}]},
{"number":43,"headRefName":"worktree-issue-15","headRefOid":"ddd","isDraft":true,"statusCheckRollup":[]},
{"number":44,"headRefName":"worktree-issue-16","headRefOid":"fault","isDraft":false,"statusCheckRollup":[]}
]"#;

const ISSUES: &str = r#"[
{"number":12,"title":"a","milestone":{"title":"Wave 3"}},
{"number":13,"title":"b","milestone":{"title":"Wave 3"}},
{"number":14,"title":"c","milestone":{"title":"Wave 3"}},
{"number":15,"title":"d","milestone":{"title":"Wave 3"}},
{"number":16,"title":"e","milestone":{"title":"Wave 3"}}
]"#;

/// A fake `gh` in `bin`. It logs each call in `bin/log`. `pr list` fails
/// as GitHub does when its `--json` names a field of [`CONNECTIONS`].
fn fake_gh(bin: &Path) -> Gh {
    let path = bin.join("gh");
    let connections = CONNECTIONS.join("|");
    let script = format!(
        r#"#!/bin/sh
dir=$(dirname "$0")
echo "$*" >> "$dir/log"
case "$*" in
'pr list '*)
    fields=$(echo "$*" | sed 's/.*--json //' | tr ',' ' ')
    for field in $fields; do
        case "$field" in
        {connections})
            echo "GraphQL: By the time this query traverses to the authors connection, it is requesting up to 1,000,000 possible nodes which exceeds the maximum limit of 500,000." >&2
            exit 1 ;;
        esac
    done
    cat "$dir/pulls" ;;
'issue list '*) cat "$dir/issues" ;;
'api repos/o/r/commits/aaa --jq .commit.message') printf 'WIP: the riff is paused\n\nThe session left the riff.\n' ;;
'api repos/o/r/commits/bbb --jq .commit.message') printf 'Add it\n\nWIP: not the first line\n' ;;
'api repos/o/r/commits/fault --jq .commit.message') echo 'HTTP 502: Bad Gateway' >&2; exit 1 ;;
*) echo "fake gh: no answer for: $*" >&2; exit 1 ;;
esac
"#
    );
    isolated::write_program(&path, script);
    std::fs::write(bin.join("pulls"), PULLS).unwrap();
    std::fs::write(bin.join("issues"), ISSUES).unwrap();
    Gh::at(path)
}

/// The calls of the fake `gh`, one for each line.
fn log(bin: &Path) -> Vec<String> {
    std::fs::read_to_string(bin.join("log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_fake_gh_refuses_a_read_of_the_commits_of_each_pull_request() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(bin.path());
    let error = gh
        .run(&["pr", "list", "--json", "number,commits"], None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("1,000,000 possible nodes"), "{error}");
    for field in CONNECTIONS {
        let json = format!("number,{field}");
        assert!(gh.run(&["pr", "list", "--json", &json], None).is_err());
    }
    assert!(
        gh.run(&["pr", "list", "--json", riff::rollout::PULL_FIELDS], None)
            .is_ok()
    );
}

#[test]
fn the_wave_reads_the_open_pull_requests_with_no_error() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(bin.path());
    let pulls = pulls(&gh, "o/r").unwrap();

    // The WIP check reads the head of an item that waits for its verify.
    let wips: Vec<(u64, bool)> = pulls.iter().map(|p| (p.number, p.wip)).collect();
    assert_eq!(
        wips,
        [
            (40, true),
            (41, false),
            (42, false),
            (43, false),
            (44, false)
        ]
    );
    // A WIP head is a build again. A fault of the read is no WIP.
    let mut waiting: Vec<u64> = in_verify(&pulls).into_iter().collect();
    waiting.sort_unstable();
    assert_eq!(waiting, [13, 14, 16]);

    // One read of the list, and one read of each head that waits for a
    // verify: not the passed, not the draft.
    let log = log(bin.path());
    assert_eq!(log.len(), 4, "{log:?}");
    assert!(log[0].starts_with("pr list --repo o/r"), "{log:?}");
    assert!(!log[0].contains("commits"), "{log:?}");
    assert_eq!(
        log[1..],
        [
            "api repos/o/r/commits/aaa --jq .commit.message",
            "api repos/o/r/commits/bbb --jq .commit.message",
            "api repos/o/r/commits/fault --jq .commit.message",
        ]
    );
}

/// The board of `riff top` reads the same pull requests.
#[test]
fn the_board_of_riff_top_counts_no_wip_head_in_verify() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(bin.path());
    let issues = issues_from_gh(&gh, "o/r").unwrap();
    assert_eq!(issues.verify, [13, 14, 16].into());
}

#[test]
fn the_wip_check_reads_the_head_commit_alone() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(bin.path());
    assert!(riff::rollout::head_wip(&gh, "o/r", "aaa"));
    assert!(!riff::rollout::head_wip(&gh, "o/r", "bbb"));
    // A fault of `gh` is no WIP, and an empty head asks `gh` nothing.
    assert!(!riff::rollout::head_wip(&gh, "o/r", "fault"));
    assert!(!riff::rollout::head_wip(&gh, "o/r", ""));
    assert_eq!(log(bin.path()).len(), 3);
}
