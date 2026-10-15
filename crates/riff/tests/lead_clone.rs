//! `riff lead start` brings the clone of the lead up to date at each
//! start (01M50RRQ9DT0JPSB358EAZRP8Z).

use std::path::Path;
use std::process::Command;

use riff::lead::{Fresh, fresh_clone};

/// Runs `git ARGS` in `dir`, and gives its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Commits a change of the file `a` in `dir`.
fn commit(dir: &Path, words: &str) -> String {
    std::fs::write(dir.join("a"), words).unwrap();
    git(dir, &["commit", "-q", "-a", "-m", words]);
    git(dir, &["rev-parse", "HEAD"])
}

/// An `origin` with one commit, and a main clone of it.
fn origin_and_main(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let origin = dir.join("origin");
    std::fs::create_dir(&origin).unwrap();
    git(&origin, &["init", "-q"]);
    std::fs::write(origin.join("a"), "one").unwrap();
    git(&origin, &["add", "a"]);
    git(&origin, &["commit", "-q", "-m", "one"]);
    let main = dir.join("main");
    git(
        dir,
        &[
            "clone",
            "-q",
            "--",
            &origin.to_string_lossy(),
            &main.to_string_lossy(),
        ],
    );
    (origin, main)
}

#[test]
fn a_second_start_gives_the_lead_a_clone_at_the_new_commit_of_origin() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = dir.path().join("lead/clone");
    assert_eq!(fresh_clone(&main, &clone).unwrap(), Fresh::Made);

    let new = commit(&origin, "two");
    assert_eq!(fresh_clone(&main, &clone).unwrap(), Fresh::Forward);
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), new);
}

#[test]
fn a_clone_with_a_local_change_stays_as_it_is_with_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let (origin, main) = origin_and_main(dir.path());
    let clone = dir.path().join("lead/clone");
    fresh_clone(&main, &clone).unwrap();
    let old = git(&clone, &["rev-parse", "HEAD"]);
    commit(&origin, "two");

    // A change that is not committed.
    std::fs::write(clone.join("a"), "mine").unwrap();
    assert_eq!(
        fresh_clone(&main, &clone).unwrap(),
        Fresh::Kept(riff::text::LEAD_CLONE_CHANGED.into())
    );
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), old);
    assert_eq!(std::fs::read_to_string(clone.join("a")).unwrap(), "mine");

    // A commit that is not on the upstream.
    let mine = commit(&clone, "mine");
    assert_eq!(
        fresh_clone(&main, &clone).unwrap(),
        Fresh::Kept(riff::text::LEAD_CLONE_AHEAD.into())
    );
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), mine);

    // A branch with no upstream.
    git(&clone, &["switch", "-q", "-c", "alone"]);
    assert_eq!(
        fresh_clone(&main, &clone).unwrap(),
        Fresh::Kept(riff::text::LEAD_CLONE_NO_UPSTREAM.into())
    );

    let note = riff::text::lead_clone_kept(&clone, riff::text::LEAD_CLONE_AHEAD);
    assert!(note.contains(&*clone.to_string_lossy()), "{note}");
    assert!(note.contains(riff::text::LEAD_CLONE_AHEAD), "{note}");
}
