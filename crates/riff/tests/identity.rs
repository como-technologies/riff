//! The naming rules against real git repositories and worktrees.

use std::path::Path;
use std::process::Command;

use riff::identity::name_in;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn repo(dir: &Path, remote: Option<&str>) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    if let Some(url) = remote {
        git(dir, &["remote", "add", "origin", url]);
    }
}

fn name(dir: &Path) -> String {
    name_in(dir, "mike", "pangolin").unwrap().to_string()
}

#[test]
fn the_main_worktree_has_no_worktree_part() {
    let tmp = tempfile::tempdir().unwrap();
    repo(
        tmp.path(),
        Some("git@github.com:como-technologies/riff.git"),
    );
    assert_eq!(
        name(tmp.path()),
        "riff://mike@pangolin/como-technologies/riff"
    );
}

#[test]
fn a_linked_worktree_adds_its_directory_name() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("riff");
    std::fs::create_dir(&main).unwrap();
    repo(&main, Some("https://github.com/como-technologies/riff.git"));
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            ".claude/worktrees/pr-23",
            "-b",
            "pr-23",
        ],
    );
    let worktree = main.join(".claude/worktrees/pr-23");
    assert_eq!(
        name(&worktree),
        "riff://mike@pangolin/como-technologies/riff#pr-23"
    );
    // A subdirectory of the worktree gives the same name.
    let sub = worktree.join("src");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(name(&sub), name(&worktree));
}

#[test]
fn a_repository_without_a_remote_is_local() {
    let tmp = tempfile::tempdir().unwrap();
    let main = tmp.path().join("scratch");
    std::fs::create_dir(&main).unwrap();
    repo(&main, None);
    assert_eq!(name(&main), "riff://mike@pangolin/local/scratch");
}

#[test]
fn a_directory_outside_git_uses_a_dash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    assert_eq!(name(&dir), "riff://mike@pangolin/-#notes");
}
