//! The place rules against real git repositories and worktrees.

use std::path::Path;
use std::process::Command;

use riff::identity::place_in;
use riff_core::name::{SessionUri, Who};

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
    let who = Who::new("mike", Some("a6cf")).unwrap();
    SessionUri::new(who, place_in(dir, "pangolin").unwrap()).to_string()
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
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
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
        "riff://mike@pangolin/como-technologies/riff?session=a6cf#pr-23"
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
    assert_eq!(
        name(&main),
        "riff://mike@pangolin/local/scratch?session=a6cf"
    );
}

#[test]
fn a_directory_outside_git_uses_a_dash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    assert_eq!(name(&dir), "riff://mike@pangolin/-?session=a6cf#notes");
}

/// R100, 01M7GMV5ZYYF94ZKQ5ECQYTJXH: the origin of a Claude Code cloud
/// session, `http://local_proxy@127.0.0.1:PORT/PATH`, gives the place of
/// the session, as before. An origin with a token, also near that form,
/// gives no place: the error names `origin` and no token.
#[test]
fn a_cloud_session_keeps_its_place_and_an_origin_with_a_token_has_none() {
    let tmp = tempfile::tempdir().unwrap();
    repo(
        tmp.path(),
        Some("http://local_proxy@127.0.0.1:8080/git/como-technologies/riff"),
    );
    assert_eq!(
        name(tmp.path()),
        "riff://mike@pangolin/como-technologies/riff?session=a6cf"
    );
    for url in isolated::ORIGINS_WITH_TOKEN {
        git(tmp.path(), &["remote", "set-url", "origin", url]);
        let error = format!("{:#}", place_in(tmp.path(), "pangolin").unwrap_err());
        assert_eq!(error, riff::text::origin_with_user(tmp.path()), "{url}");
    }
}
