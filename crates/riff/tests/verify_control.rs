//! The start text of a verify gives the control
//! (01M5174ASF376JDNBQ4423TTAR, 01M7CF4AV7X0Q30DNY1XVY6HTE, #1871). The test runs the `git` commands
//! of the step with a real git, in a clone like the clone of a verify:
//! a clone of `origin` at the head of the pull request, detached. They
//! go to the base commit of the pull request, not to the tip of `main`,
//! and back to the head, and they change no file. "Verify finished
//! work" in the book says that the start text gives the control.

use std::path::Path;
use std::process::Command;

/// The pull request of the test.
const PULL: u64 = 40;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "advice.detachedHead=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Commits the file `name` with the text `text` in `clone`, and gives
/// the commit.
fn commit(clone: &Path, name: &str, text: &str) -> String {
    std::fs::write(clone.join(name), text).unwrap();
    git(clone, &["add", name]);
    git(clone, &["commit", "-q", "-m", name]);
    git(clone, &["rev-parse", "HEAD"])
}

/// The text of `text` from `start` to the first `end` after it.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("no {start:?} in {text}"));
    let rest = &text[from..];
    let to = rest
        .find(end)
        .unwrap_or_else(|| panic!("no {end:?} after {start:?} in {text}"));
    &rest[..to]
}

/// Runs the `git` command `line` of the start text in `dir`.
fn run(dir: &Path, line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(words.first(), Some(&"git"), "{line}");
    git(dir, &words[1..])
}

#[test]
fn the_control_of_a_verify_runs_at_the_base_commit_and_goes_back_to_the_head() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    git(root, &["init", "-q", "--bare", "origin.git"]);
    git(root, &["clone", "-q", "origin.git", "work"]);
    let work = root.join("work");
    let base = commit(&work, "guard", "no guard\n");
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    // The pull request fixes the fault, and main moves after it.
    git(&work, &["checkout", "-q", "-b", "fix"]);
    let head = commit(&work, "guard", "the guard\n");
    git(
        &work,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/pull/{PULL}/head"),
        ],
    );
    git(&work, &["checkout", "-q", "main"]);
    let tip = commit(&work, "other", "other\n");
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    assert_ne!(base, tip);

    // The clone of a verify: at the head of the pull request, detached.
    git(root, &["clone", "-q", "origin.git", "verify"]);
    let clone = root.join("verify");
    git(
        &clone,
        &["fetch", "-q", "origin", &format!("refs/pull/{PULL}/head")],
    );
    git(&clone, &["checkout", "-q", "--detach", &head]);

    let checks = ["CI".to_owned()];
    let text = riff::text::wave_verify_start("o/r", "main", 12, PULL, &head, &checks);
    let step = between(&text, "\n6. Run a control", "\n7. ");
    assert!(
        step.contains("Never edit the code for a control, also not for a moment"),
        "{step}"
    );
    assert!(
        step.contains("Say in your result which way you used."),
        "{step}"
    );

    let merge_base = between(step, "git merge-base", " prints BASE.");
    let found = run(&clone, merge_base);
    assert_eq!(
        found, base,
        "{merge_base} found the tip of main or another commit"
    );
    let to_base = between(step, "git checkout --detach BASE", ",");
    run(&clone, &to_base.replace("BASE", &found));
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), base);
    let guard = std::fs::read_to_string(clone.join("guard")).unwrap();
    assert_eq!(guard, "no guard\n", "the base commit shows the fault");

    let back = between(step, "then run git checkout", " before your result.");
    let back = back.strip_prefix("then run ").unwrap();
    assert_eq!(back, format!("git checkout --detach {head}"));
    run(&clone, back);
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&clone, &["status", "--porcelain"]),
        "",
        "the control changed a file"
    );
    let guard = std::fs::read_to_string(clone.join("guard")).unwrap();
    assert_eq!(guard, "the guard\n");

    let part = crate::book::part("how-it-works.md", "Verify finished work");
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "The verifier changes no code, also not for a moment.",
        "The start text of each verify gives the control in a step of its own.",
        "The verifier runs the control at the base commit of the pull request, the merge \
         base with `main`, then checks out the commit again.",
        "reads and runs a test of the guard on a sample, uses the control of the author \
         on the issue, or checks by a read. Its result says which way it used.",
    ] {
        assert!(flat.contains(words), "no {words:?} in {part}");
    }
}
