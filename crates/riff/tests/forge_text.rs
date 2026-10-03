//! Each text of the forge that riff prints, stores or sends goes
//! through `text::forge` (01M3ZRQY6YQ8QAKZPGWH1XD6WW, #443).
//!
//! A fake `gh` puts an escape code and a line break in each text that
//! it gives: titles, milestones, branches, commits, checks, URLs and
//! errors. Each place where riff reads it must give none of them back.
//! `riff usage` has its own test in `usage.rs`.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use riff::pr::{Gh, Verdict};
use riff::rollout::{Milestone, Pull};
use riff::top::Issues;

/// The bad part of each text: it sets the title of the terminal,
/// clears the line, and breaks the line.
const BAD: &str = r"\u001b]0;owned\u0007\u001b[2K\r\n";

/// A fake `gh` in `bin`: it answers each call with JSON whose texts
/// hold [`BAD`]. It copies its stdin to `bin/body`. `label list`
/// fails with [`BAD`] in its error.
fn fake_gh(bin: &Path) -> Gh {
    let path = bin.join("gh");
    let script = r#"#!/bin/sh
dir=$(dirname "$0")
case "$*" in *'--body-file -'*) cat > "$dir/body" ;; esac
case "$1 $2" in
'api repos/acme/app/milestones?state=all&per_page=100')
    printf '%s' '[{"title":"Wave 3: NBADew","open_issues":0,"closed_issues":2}]' ;;
'api repos/acme/app/milestones?state=open&per_page=100')
    printf '%s' '[{"title":"Wave 4: CloBADud"}]' ;;
'issue list') printf '%s' '[{"number":7,"title":"Release v1.2.3","milestone":{"title":"Wave 4: NBADew"}}]' ;;
'issue view') printf '%s' '{"number":7,"state":"OPEN","milestone":{"title":"Wave 4: NBADew"}}' ;;
'pr list') printf '%s' '[{"number":39,"title":"Old","headRefName":"worktree-issue-6BAD","headRefOid":"0","isDraft":false,"statusCheckRollup":[]},{"number":40,"title":"Release v1.2.3","headRefName":"worktree-issue-7","headRefOid":"abc1234BADdef","isDraft":false,"statusCheckRollup":[{"context":"riff/verify","state":"FAILURE","targetUrl":"https://e.test/cBAD"}],"milestone":{"title":"Wave 4: NBADew"}}]' ;;
'pr view') printf '%s' '{"state":"MERGED","mergeCommit":{"oid":"9f8eBAD7d6c"},"headRefOid":"abc1234BADdef","body":"Closes #7\n\nIssue: #7\nMilestone: Wave 4\n"}' ;;
'pr checks') printf '%s' '[{"name":"GateBAD","bucket":"pass"}]' ;;
'pr create') printf 'https://e.test/pull/41\n' ;;
'pr comment') printf 'https://e.test/c\033]0;owned\007\033[2K\r\nend\n' ;;
'run list') printf '%s' '[{"conclusion":"success"}]' ;;
'label list') printf '%s\n' 'no labelsBAD' >&2; exit 1 ;;
esac
exit 0
"#
    .replace("BAD", BAD);
    // The JSON escapes of BAD are for the JSON answers. The error is no
    // JSON: it gets the real characters.
    let script = script.replace(
        &format!("no labels{BAD}"),
        "no labels\u{1b}]0;owned\u{7}\u{1b}[2K\r\nend",
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Gh::at(path)
}

/// Fails when `text` has a control character.
#[track_caller]
fn clean(text: &str) {
    assert!(!text.chars().any(char::is_control), "{text:?}");
    assert!(!text.contains("]0;"), "{text:?}");
}

#[test]
fn each_reply_of_the_forge_that_riff_reads_has_no_control_character() {
    let bin = tempfile::tempdir().unwrap();
    let gh = fake_gh(bin.path());

    // The wave line of the lead and the rollout.
    let open: Vec<Milestone> = gh
        .json(&["api", "repos/acme/app/milestones?state=open&per_page=100"])
        .unwrap();
    assert_eq!(open[0].title, "Wave 4: Clo  ud");
    assert_eq!(riff::rollout::current_wave(&open), Some("Wave 4: Clo  ud"));

    // The notes of the rollout and the start context: the pull request
    // of a claim.
    let pulls: Vec<Pull> = riff::rollout::pulls(&gh, "acme/app").unwrap();
    assert_eq!(pulls[0].branch, "worktree-issue-6  ");
    assert_eq!(pulls[1].head, "abc1234  def");
    let line = riff::rollout::claim_line("issue-7", &pulls).unwrap();
    clean(&line);
    assert!(
        line.contains("for commit abc1234: https://e.test/c  "),
        "{line}"
    );

    // The board of `riff top`: titles, the wave and the verifies.
    let issues = Issues::parse(
        &r#"[{"number":7,"title":"FixBAD it","milestone":{"title":"Wave 4: NBADew"}}]"#
            .replace("BAD", BAD),
    )
    .unwrap();
    assert_eq!(issues.titles[&7], "Fix   it");
    assert_eq!(issues.wave, Some(("Wave 4: N  ew".into(), vec![7])));

    // The wave of the end of a wave, in the facts of a compact.
    let wave = riff::compact::forge::released_wave(&gh, "acme/app").unwrap();
    assert_eq!(wave.as_deref(), Some("Wave 3: N  ew"));

    // `riff pr wait`: the merge commit.
    let merged = riff::pr::wait(&gh, 40, std::time::Duration::ZERO).unwrap();
    assert_eq!(merged, "9f8e  7d6c");

    // `riff verify`: the commit and the URL go into the result.
    let reported = riff::pr::report(&gh, "acme/app", 40, "abc1234", Verdict::Pass, "ok").unwrap();
    clean(&reported.commit);
    assert_eq!(reported.url, "https://e.test/c  end");
    let comment = std::fs::read_to_string(bin.path().join("body")).unwrap();
    assert!(!comment.contains('\u{1b}'), "{comment:?}");

    // `riff pr open`: a milestone with a control character would go
    // into the body of the pull request. It is refused, and nothing is
    // sent.
    std::fs::remove_file(bin.path().join("body")).unwrap();
    let refused = riff::pr::open(&gh, "Fix it", "Fix it.\n", 7, false).unwrap_err();
    let refused = format!("{refused:#}");
    clean(&refused);
    assert!(refused.contains("has a control character"), "{refused}");
    assert!(!bin.path().join("body").exists());

    // An error of `gh`.
    let error = format!("{:#}", gh.run(&["label", "list"], None).unwrap_err());
    clean(&error);
    assert!(error.ends_with("no labels  end"), "{error}");
}
