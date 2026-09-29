//! Pull requests on GitHub: the project settings of Claude Code deny a
//! push to `main` (01M3JFEXJG2D651PWA30DNRGWF), and `just github` sets up
//! the repository (01M3JFEXG85AJK8ZE8N807EQVB) with a ruleset that has
//! no bypass (01M3JN4QQCM0GXK9BCGXVS2YC7), and a ruleset on the release
//! tags (01M3MRMB0AJVPD952AQYD7X1RN). A fake `gh` on `PATH`
//! writes each call and its input to a log.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rules of `permissions.KIND` in `.claude/settings.json`, as the
/// patterns inside `Bash(...)`.
fn rules(kind: &str) -> Vec<String> {
    let text = std::fs::read_to_string(repo().join(".claude/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&text).unwrap();
    settings["permissions"][kind]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let r = r.as_str().unwrap();
            r.strip_prefix("Bash(")
                .and_then(|r| r.strip_suffix(')'))
                .unwrap_or_else(|| panic!("not a Bash rule: {r}"))
                .to_owned()
        })
        .collect()
}

/// True when `command` matches `pattern`, where `*` matches any text, as
/// in the Bash rules of Claude Code.
fn matches(pattern: &str, command: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, rest) = parts.split_first().unwrap();
    let Some(mut left) = command.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return left.is_empty();
    };
    for part in middle {
        match left.find(part) {
            Some(i) => left = &left[i + part.len()..],
            None => return false,
        }
    }
    left.ends_with(last)
}

fn denied(command: &str) -> bool {
    rules("deny").iter().any(|p| matches(p, command))
}

fn allowed(command: &str) -> bool {
    !denied(command) && rules("allow").iter().any(|p| matches(p, command))
}

#[test]
fn the_matcher_follows_the_wildcard_rules() {
    assert!(matches("git push * main", "git push origin main"));
    assert!(matches("gh pr create *", "gh pr create --title x"));
    assert!(!matches("git push * main", "git push origin maintenance"));
    assert!(matches("a*b*c", "a1b2c"));
    assert!(!matches("a*b*c", "a1c2b"));
    assert!(matches("exact", "exact"));
    assert!(!matches("exact", "exact more"));
}

#[test]
fn the_settings_deny_a_push_to_main_and_an_admin_merge() {
    for rule in rules("allow") {
        assert!(!rule.contains("main"), "an allow rule names main: {rule}");
    }
    for command in [
        "git push origin HEAD:main",
        "git push origin main",
        "git push -f origin main",
        "git push --force origin HEAD:main",
        "git push origin HEAD:main --force",
        "git push origin HEAD:refs/heads/main",
        "git push origin worktree-issue-12:main",
        "gh pr merge 40 --admin",
        "gh pr merge --admin --squash 40",
    ] {
        assert!(denied(command), "not denied: {command}");
    }
    for command in [
        "git push -u origin HEAD",
        "git push origin --delete worktree-issue-12",
        "git push origin maintenance",
    ] {
        assert!(!denied(command), "denied: {command}");
    }
}

#[test]
fn the_settings_allow_the_steps_of_a_pull_request() {
    for command in [
        "gh pr create --title \"Show the wave\" --milestone \"Wave 3\" --body-file pr.md",
        "gh pr merge 40 --auto --squash",
        "gh pr comment 40 --body-file result.md",
        "gh api repos/como-technologies/riff/statuses/1a2b3c4 -f state=success -f context=riff/verify",
        "riff pr open --title \"Show the wave\" --file summary.md",
        "riff pr wait 40",
        "riff verify pass 40 --file result.md",
        "riff verify fail 40 --file result.md",
    ] {
        assert!(allowed(command), "not allowed: {command}");
    }
}

const FAKE_GH: &str = r#"#!/bin/sh
dir=$(dirname "$0")
echo "gh $*" >> "$dir/log"
case "$*" in
  *"--input -"*) cat >> "$dir/log"; echo ;;
  *'/rulesets --jq'*'"main"'*) cat "$dir/ruleset-main" 2>/dev/null ;;
  *'/rulesets --jq'*'"releases"'*) cat "$dir/ruleset-releases" 2>/dev/null ;;
esac
exit 0
"#;

/// Runs `deploy/github.sh` with a fake `gh`. `ruleset` is the ID of the
/// ruleset `main` that exists, if any; the ruleset `releases` then
/// exists with the ID 8. Returns stdout and the log.
fn setup(ruleset: Option<&str>) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let gh = dir.path().join("gh");
    std::fs::write(&gh, FAKE_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    if let Some(id) = ruleset {
        std::fs::write(dir.path().join("ruleset-main"), format!("{id}\n")).unwrap();
        std::fs::write(dir.path().join("ruleset-releases"), "8\n").unwrap();
    }
    let out = Command::new(repo().join("deploy/github.sh"))
        .arg("owner/repo")
        .env("GH", &gh)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
    (String::from_utf8(out.stdout).unwrap(), log)
}

/// The JSON of the ruleset `name` in the log.
fn ruleset(log: &str, name: &str) -> serde_json::Value {
    let mut stream = log.match_indices("\n{").map(|(at, _)| {
        serde_json::Deserializer::from_str(&log[at + 1..])
            .into_iter::<serde_json::Value>()
            .next()
            .unwrap()
            .unwrap()
    });
    stream
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no ruleset {name}:\n{log}"))
}

#[test]
fn just_github_sets_the_repository_and_makes_the_ruleset() {
    let (out, log) = setup(None);
    assert!(
        out.contains("Made the ruleset main of owner/repo."),
        "{out}"
    );
    let calls: Vec<&str> = log.lines().filter(|l| l.starts_with("gh ")).collect();
    assert_eq!(
        calls,
        [
            "gh api -X PATCH repos/owner/repo -F allow_auto_merge=true -F allow_squash_merge=true \
             -F allow_merge_commit=false -F allow_rebase_merge=false \
             -f squash_merge_commit_title=PR_TITLE -f squash_merge_commit_message=PR_BODY \
             -F delete_branch_on_merge=true",
            "gh api repos/owner/repo/rulesets --jq .[] | select(.name == \"main\") | .id",
            "gh api -X POST repos/owner/repo/rulesets --input -",
            "gh api repos/owner/repo/rulesets --jq .[] | select(.name == \"releases\") | .id",
            "gh api -X POST repos/owner/repo/rulesets --input -",
        ]
    );
    let rules = ruleset(&log, "main");
    assert_eq!(rules["name"], "main");
    assert_eq!(rules["enforcement"], "active");
    assert_eq!(
        rules["conditions"]["ref_name"]["include"],
        serde_json::json!(["~DEFAULT_BRANCH"])
    );
    // No actor can bypass the ruleset (01M3JN4QQCM0GXK9BCGXVS2YC7).
    assert_eq!(rules["bypass_actors"], serde_json::json!([]));
    let rule = |kind: &str| {
        rules["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["type"] == kind)
            .unwrap_or_else(|| panic!("no rule {kind}"))
            .clone()
    };
    rule("deletion");
    rule("non_fast_forward");
    let pr = rule("pull_request")["parameters"].clone();
    assert_eq!(pr["required_approving_review_count"], 0);
    assert_eq!(pr["allowed_merge_methods"], serde_json::json!(["squash"]));
    let checks = rule("required_status_checks")["parameters"].clone();
    assert_eq!(checks["strict_required_status_checks_policy"], false);
    assert_eq!(
        checks["required_status_checks"],
        serde_json::json!([{ "context": "Gate" }, { "context": "Hygiene" }, { "context": "riff/verify" }])
    );
}

#[test]
fn just_github_again_updates_the_same_ruleset() {
    let (out, log) = setup(Some("7"));
    assert!(
        out.contains("Updated the ruleset main (7) of owner/repo."),
        "{out}"
    );
    assert!(
        log.contains("gh api -X PUT repos/owner/repo/rulesets/7 --input -"),
        "{log}"
    );
    assert!(
        log.contains("gh api -X PUT repos/owner/repo/rulesets/8 --input -"),
        "{log}"
    );
    assert!(!log.contains("-X POST"), "{log}");
    assert_eq!(ruleset(&log, "main")["name"], "main");
}

/// 01M3MRMB0AJVPD952AQYD7X1RN: only the repository admin role creates,
/// moves or deletes a release tag `v*`.
#[test]
fn just_github_lets_only_an_admin_make_a_release_tag() {
    let (out, log) = setup(None);
    assert!(
        out.contains("Made the ruleset releases of owner/repo."),
        "{out}"
    );
    let rules = ruleset(&log, "releases");
    assert_eq!(rules["target"], "tag");
    assert_eq!(rules["enforcement"], "active");
    assert_eq!(
        rules["conditions"]["ref_name"]["include"],
        serde_json::json!(["refs/tags/v*"])
    );
    assert_eq!(
        rules["bypass_actors"],
        serde_json::json!([{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }])
    );
    let kinds: Vec<&str> = rules["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["creation", "update", "deletion"]);
}

#[test]
fn the_justfile_runs_the_setup() {
    let justfile = std::fs::read_to_string(repo().join("justfile")).unwrap();
    assert!(
        justfile.contains("github REPO=\"como-technologies/riff\":\n    deploy/github.sh {{REPO}}"),
        "{justfile}"
    );
}
