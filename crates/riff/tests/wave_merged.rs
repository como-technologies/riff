//! The free work counts an open issue whose pull request merged with
//! `Refs #N` as merged, also before it has the comment `Merged in`
//! (01M6XSSCTA3E1GJ8EC9646PP1R, #1754): the look of the wave reads the
//! open issues and the merged pull requests from a fake `gh`. The look
//! that finds such an issue with no comment adds it one time.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use riff::pr::Gh;
use riff::rollout::Merge;
use riff::wave;

const REPO: &str = "o/r";

/// The merge commit of pull request 40.
const COMMIT: &str = "1a2b3c4d5e6f";

/// A fake `gh`: the milestones, no open pull request, the file `merged`
/// for `pr list --state merged`, and the open issues. After a comment
/// (the file `said`), `issue list` and `issue view` give the issue 12
/// with it. Each comment goes in the file `commented`.
const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
  api*)
    case "$2" in
      repos/o/r) echo '{"default_branch":"main"}' ;;
      */rules/branches/main*) echo '[]' ;;
      */milestones*) echo '[{"title":"Backlog"},{"title":"Wave 1"}]' ;;
      *) exit 1 ;;
    esac ;;
  "pr list")
    case "$*" in
      *"--state merged"*) cat "$dir/merged" ;;
      *) echo '[]' ;;
    esac ;;
  "issue list")
    if [ -e "$dir/said" ]; then cat "$dir/issues-said"; else cat "$dir/issues"; fi ;;
  "issue view")
    if [ -e "$dir/said" ]; then cat "$dir/view-said"; else echo '{"comments":[]}'; fi ;;
  "issue comment")
    echo "$*" >> "$dir/commented"
    cat > "$dir/said" ;;
  *) exit 1 ;;
esac
"#;

/// The open issue `number` of the wave, with the comments `comments`.
fn issue(number: u64, comments: &[&str]) -> serde_json::Value {
    let comments: Vec<serde_json::Value> = comments
        .iter()
        .map(|body| serde_json::json!({ "body": body }))
        .collect();
    serde_json::json!({
        "number": number,
        "title": format!("item {number}"),
        "body": "Done when: it works.",
        "comments": comments,
        "milestone": { "title": "Wave 1" },
        "labels": [],
    })
}

/// The fake `gh` in `dir`: the open issues 12 and 13, and the merged pull
/// request 40 of issue 12 with `Refs #12`. Issue 12 has no comment
/// `Merged in`.
fn forge(dir: &Path) -> Gh {
    let path = dir.join("gh");
    isolated::write_program(&path, FAKE_GH);
    let put = |name: &str, value: serde_json::Value| {
        std::fs::write(dir.join(name), value.to_string()).unwrap();
    };
    put(
        "issues",
        serde_json::json!([issue(12, &[]), issue(13, &[])]),
    );
    let said = riff::text::wave_merged(40, COMMIT);
    put(
        "issues-said",
        serde_json::json!([issue(12, &[&said]), issue(13, &[])]),
    );
    put(
        "view-said",
        serde_json::json!({ "comments": [{ "body": said }] }),
    );
    put(
        "merged",
        serde_json::json!([
            {
                "number": 40,
                "state": "MERGED",
                "headRefName": "worktree-issue-12",
                "body": "Refs #12\n\nThe summary.\n\nIssue: #12\nMilestone: Wave 1\n",
                "mergeCommit": { "oid": COMMIT },
            },
            // An older merge of issue 13, with `Closes #13`: not one of
            // R194, so 13 stays free work.
            {
                "number": 30,
                "state": "MERGED",
                "headRefName": "worktree-issue-13",
                "body": "Closes #13\n\nIssue: #13\n",
                "mergeCommit": { "oid": "9f8e7d6" },
            },
        ]),
    );
    Gh::at(path)
}

/// The comments that the fake `gh` got.
fn commented(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("commented"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The look of the wave from `gh`, with no claim and no hold.
fn look(gh: &Gh) -> wave::Look {
    wave::look_on(gh, REPO, BTreeSet::new(), BTreeSet::new()).unwrap()
}

#[test]
fn an_issue_whose_pull_request_merged_with_refs_is_no_free_work_and_gets_the_comment_once() {
    let dir = tempfile::tempdir().unwrap();
    let gh = forge(dir.path());

    // The merge left issue 12 open, with no comment: it is no free work,
    // for the wave and for the rollout of the lead.
    let first = look(&gh);
    assert_eq!(first.free, ["issue-13"]);
    let merge = Merge {
        pull: 40,
        commit: COMMIT.into(),
    };
    assert_eq!(first.merged, BTreeMap::from([(12, merge)]));
    let none = HashSet::new();
    let work = riff::rollout::free_work(&gh, REPO, &none, &none).unwrap();
    assert_eq!(work, ["issue-13"]);

    // The look adds the comment, as the wave does with `World::merged_in`.
    for (issue, merge) in &first.merged {
        riff::daemon::sweep::merged_in_on(&gh, *issue, merge.pull, &merge.commit).unwrap();
    }
    assert_eq!(commented(dir.path()), ["issue comment 12 --body-file -"]);
    let said = std::fs::read_to_string(dir.path().join("said")).unwrap();
    assert_eq!(said.trim_end(), riff::text::wave_merged(40, COMMIT));

    // The next look finds the comment: nothing more to add, and 12 is
    // still no free work. A second call adds no second comment.
    let next = look(&gh);
    assert!(next.merged.is_empty(), "{:?}", next.merged);
    assert_eq!(next.free, ["issue-13"]);
    riff::daemon::sweep::merged_in_on(&gh, 12, 40, COMMIT).unwrap();
    assert_eq!(commented(dir.path()).len(), 1, "one comment");
}
