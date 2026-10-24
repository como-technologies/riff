//! A build that ends with no pushed commit: riff saves its work as a WIP
//! commit on the branch of the item (01M5P26VSA7AA99K1RYG83R8J9), over
//! real git.

use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "riff test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "riff test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn a_clone_with_changes_and_no_pushed_commit_gets_a_wip_commit_on_its_branch() {
    let root = tempfile::tempdir().unwrap();
    let (origin, clone) = (root.path().join("origin.git"), root.path().join("clone"));
    git(
        root.path(),
        &["init", "-q", "--bare", "-b", "main", "origin.git"],
    );
    git(
        root.path(),
        &["clone", "-q", origin.to_str().unwrap(), "clone"],
    );
    git(&clone, &["config", "user.name", "riff test"]);
    git(&clone, &["config", "user.email", "test@example.com"]);
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "start"]);
    git(&clone, &["push", "-q", "origin", "HEAD:main"]);
    git(&clone, &["remote", "set-head", "origin", "main"]);
    git(&clone, &["checkout", "-q", "-b", "worktree-issue-1"]);

    // A clean clone has nothing to save.
    let branch = "worktree-issue-1";
    assert_eq!(riff::leave::save(&clone, branch).unwrap(), None);

    std::fs::write(clone.join("work.md"), "work\n").unwrap();
    let (pushed, commit) = riff::leave::save(&clone, branch).unwrap().unwrap();
    assert_eq!(pushed, branch);
    assert_eq!(
        git(&origin, &["rev-parse", "--short", branch]),
        commit,
        "origin has the commit"
    );
    assert!(git(&origin, &["log", "-1", "--format=%s", branch]).starts_with("WIP"));

    // The branch is on origin: a new change stays where it is.
    std::fs::write(clone.join("more.md"), "more\n").unwrap();
    assert_eq!(riff::leave::save(&clone, branch).unwrap(), None);
}
