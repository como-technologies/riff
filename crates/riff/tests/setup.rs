//! `riff setup` adds the Claude Code permission rules of riff to the
//! project settings, and `riff setup --check` names the missing ones
//! (01M3Q53RNDJBDHVDFHJ9HCX9S1). The tests use a temporary HOME: they
//! never touch the real Claude settings.

use crate::book;

use isolated::Isolated;
use std::path::Path;
use std::process::Output;

/// A git clone of `acme/app` on GitHub, with the default branch `trunk`.
fn clone() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "trunk"][..],
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    ] {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }
    dir
}

/// `riff setup ARGS` in `dir`, with the HOME of `env`.
fn setup(env: &Isolated, dir: &Path, args: &[&str]) -> Output {
    env.riff()
        .arg("setup")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn settings(dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn setup_writes_the_rules_once_and_check_finds_them() {
    let env = Isolated::new();
    let repo = clone();
    let sub = repo.path().join("src");
    std::fs::create_dir(&sub).unwrap();

    let check = setup(&env, &sub, &["--check"]);
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    assert!(stdout(&check).contains("allow Bash(riff *)"), "{check:?}");
    assert!(stdout(&check).ends_with("run: riff setup\n"), "{check:?}");
    assert!(!repo.path().join(".claude").exists());

    let first = setup(&env, &sub, &[]);
    assert!(first.status.success(), "{first:?}");
    assert!(stdout(&first).starts_with("Added "), "{first:?}");
    let value = settings(repo.path());
    let allow = value["permissions"]["allow"].as_array().unwrap();
    let deny = value["permissions"]["deny"].as_array().unwrap();
    for rule in [
        "mcp__plugin_riff_riff",
        "mcp__riff",
        "Bash(riff)",
        "Bash(riff *)",
        "Bash(gh pr create *)",
        "Bash(gh pr merge * --auto --squash)",
        "Bash(gh api repos/acme/app/statuses/*)",
    ] {
        assert!(allow.iter().any(|r| r == rule), "{rule}: {value}");
    }
    for rule in [
        "Bash(git push * trunk)",
        "Bash(git push *:refs/heads/trunk)",
        "Bash(gh pr merge *--admin*)",
    ] {
        assert!(deny.iter().any(|r| r == rule), "{rule}: {value}");
    }

    let text = std::fs::read_to_string(repo.path().join(".claude/settings.json")).unwrap();
    let again = setup(&env, &sub, &[]);
    assert!(again.status.success(), "{again:?}");
    assert!(stdout(&again).contains("riff changed nothing"), "{again:?}");
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".claude/settings.json")).unwrap(),
        text
    );
    let check = setup(&env, &sub, &["--check"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    assert!(!env.home().join(".claude/settings.json").exists());
}

#[test]
fn setup_keeps_the_rules_and_keys_that_are_there() {
    let env = Isolated::new();
    let repo = clone();
    std::fs::create_dir(repo.path().join(".claude")).unwrap();
    std::fs::write(
        repo.path().join(".claude/settings.json"),
        r#"{"model": "opus", "permissions": {"allow": ["Bash(ls)"], "ask": ["Bash(rm *)"]}}"#,
    )
    .unwrap();
    let out = setup(&env, repo.path(), &[]);
    assert!(out.status.success(), "{out:?}");
    let value = settings(repo.path());
    assert_eq!(value["model"], "opus");
    assert_eq!(value["permissions"]["allow"][0], "Bash(ls)");
    assert_eq!(value["permissions"]["ask"][0], "Bash(rm *)");
}

#[test]
fn a_rule_in_the_user_settings_counts_as_there() {
    let env = Isolated::new();
    let repo = clone();
    let user = env.home().join(".claude/settings.json");
    std::fs::create_dir_all(user.parent().unwrap()).unwrap();
    std::fs::write(&user, r#"{"permissions": {"allow": ["Bash(riff *)"]}}"#).unwrap();
    let out = setup(&env, repo.path(), &[]);
    assert!(out.status.success(), "{out:?}");
    assert!(!stdout(&out).contains("allow Bash(riff *)"), "{out:?}");
    let value = settings(repo.path());
    let allow = value["permissions"]["allow"].as_array().unwrap();
    assert!(!allow.iter().any(|r| r == "Bash(riff *)"), "{value}");
}

#[test]
fn settings_that_are_not_json_stay_as_they_are() {
    let env = Isolated::new();
    let repo = clone();
    let path = repo.path().join(".claude/settings.json");
    std::fs::create_dir(repo.path().join(".claude")).unwrap();
    std::fs::write(&path, "{ not json").unwrap();
    let out = setup(&env, repo.path(), &[]);
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
}

#[test]
fn this_repository_has_each_riff_rule() {
    let env = Isolated::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = setup(&env, &root, &["--check"]);
    assert!(out.status.success(), "{}", stdout(&out));
}

#[test]
fn the_book_tells_how_to_add_and_check_the_rules() {
    let commands = book::commands_of("start-a-riff.md");
    assert!(commands.iter().any(|c| c == "riff setup"), "{commands:?}");
    assert!(
        commands.iter().any(|c| c == "riff setup --check"),
        "{commands:?}"
    );
}
