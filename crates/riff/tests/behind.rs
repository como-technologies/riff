//! The start hook tells a session when its clone is behind `origin`
//! (01M3JN21T9C5GX6VX8N032JYWE). With no remote, or a remote that
//! cannot be reached, it adds no line, exits 0, and ends within its
//! time limit (01M3JN21WDXWTHDKXKQ80ZPYPK).

use isolated::Isolated;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use riff::hook::{FETCH_WAIT, STATE_WAIT};

/// Runs git with no global or system config, for example no commit
/// signing.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A bare remote `origin.git` with one commit on `main`, and a clone of
/// it in `clone`.
fn remote_and_clone(root: &Path) {
    git(root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "seed"]);
    let seed = root.join("seed");
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "one"]);
    git(&seed, &["push", "-q", "origin", "HEAD:main"]);
    git(root, &["clone", "-q", "origin.git", "clone"]);
}

/// One more commit on `main` in the remote.
fn push_one(root: &Path) {
    let seed = root.join("seed");
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "two"]);
    git(&seed, &["push", "-q", "origin", "HEAD:main"]);
}

/// The context of the start hook in `dir`, and how long the hook ran.
fn context(dir: &Path, env: &[(&str, &str)]) -> (String, Duration) {
    let run = tempfile::tempdir().unwrap();
    let mut cmd = Isolated::shared().riff();
    cmd.args(["hook", "session-start"])
        .current_dir(dir)
        .env("RIFF_HOME", run.path())
        .env("RIFF_USER", "mike")
        .env("RIFF_HOST", "pangolin")
        .env("RIFF_SERVER", "http://127.0.0.1:9")
        .env_remove("RIFF_SESSION")
        .env("CLAUDE_CODE_SESSION_ID", "a6cf")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let start = Instant::now();
    let out = cmd.output().unwrap();
    let took = start.elapsed();
    assert!(out.status.success(), "{out:?}");
    let out: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned();
    (context, took)
}

/// The hook reads the state of the riff and fetches at the same time.
/// So it ends within the longer of the two limits, plus a margin for the
/// process.
fn within_limits(took: Duration) {
    let limit = STATE_WAIT.max(FETCH_WAIT) + Duration::from_secs(2);
    assert!(took < limit, "the hook took {took:?}");
}

#[test]
fn a_clone_one_commit_behind_gets_the_line() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    push_one(root.path());
    let clone = root.path().join("clone");
    let (context, took) = context(&clone, &[]);
    assert!(
        context.contains("This clone is 1 commit behind origin/main."),
        "{context}"
    );
    assert!(context.contains("pull --ff-only"), "{context}");
    assert!(context.contains("Do not pull yourself"), "{context}");
    within_limits(took);
}

#[test]
fn a_worktree_names_the_main_worktree_for_the_pull() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    push_one(root.path());
    let clone = root.path().join("clone");
    git(&clone, &["worktree", "add", "-q", "-b", "work", "../work"]);
    let (context, _) = context(&root.path().join("work"), &[]);
    let main = clone.canonicalize().unwrap();
    assert!(
        context.contains(&format!("git -C {} pull --ff-only", main.display())),
        "{context}"
    );
}

#[test]
fn a_clone_that_is_up_to_date_gets_no_line() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    let (context, took) = context(&root.path().join("clone"), &[]);
    assert!(!context.contains("behind origin"), "{context}");
    within_limits(took);
}

#[test]
fn a_clone_after_the_pull_gets_no_line() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    push_one(root.path());
    let clone = root.path().join("clone");
    assert!(context(&clone, &[]).0.contains("behind origin"));
    git(&clone, &["pull", "-q", "--ff-only"]);
    assert!(!context(&clone, &[]).0.contains("behind origin"));
}

#[test]
fn a_repository_with_no_remote_gets_no_line() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q", "-b", "main", "repo"]);
    let (context, took) = context(&root.path().join("repo"), &[]);
    assert!(!context.contains("behind origin"), "{context}");
    within_limits(took);
}

#[test]
fn a_remote_that_is_gone_gets_no_line() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    push_one(root.path());
    let clone = root.path().join("clone");
    git(
        &clone,
        &["remote", "set-url", "origin", "/nonexistent/origin.git"],
    );
    let (context, took) = context(&clone, &[]);
    assert!(!context.contains("behind origin"), "{context}");
    within_limits(took);
}

/// The remote does not answer: the ssh command of git sleeps. The hook
/// stops the fetch at its time limit.
#[test]
fn a_remote_that_does_not_answer_gets_no_line_in_time() {
    let root = tempfile::tempdir().unwrap();
    remote_and_clone(root.path());
    push_one(root.path());
    let clone = root.path().join("clone");
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "ssh://nowhere.invalid/origin.git",
        ],
    );
    let (context, took) = context(&clone, &[("GIT_SSH_COMMAND", "sleep 30; :")]);
    assert!(!context.contains("behind origin"), "{context}");
    assert!(took >= FETCH_WAIT, "the fetch did not wait: {took:?}");
    within_limits(took);
}
