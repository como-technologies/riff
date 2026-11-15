//! The verify of a fix head (#2173): a head after a FAIL of the same pull
//! request (01M7GDQWDA1Z08V4EDMP255ZF1, 01M7GDQWGCBVJE6S4MJDEJ6P8T). Its
//! start text says the rule and gives the head of the last verify. The
//! start text of a first verify stays as it was. The test runs the `git`
//! commands of the diff with a real git, in a clone like the clone of a
//! verify, also after a rebase. "What fails the verify of a fix head" in
//! the book says the rule.

use std::path::Path;
use std::process::Command;

use riff::text::{
    VERIFY_FIRST_RULE, VERIFY_NOT_SHOWN, verify_fix_diff, verify_fix_rule, wave_verify_start,
};

/// The pull request of the test.
const PULL: u64 = 40;

/// The head of the last verify, which failed.
const LAST: &str = "9f8e7d6c5b4a";

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

/// True when the git command `args` succeeds in `dir`.
fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
        .status
        .success()
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
    let rest = &text[from + start.len()..];
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

/// 01M7GDQWDA1Z08V4EDMP255ZF1, 01M7GDQWGCBVJE6S4MJDEJ6P8T: the start
/// text of a verify of a fix head names the rule and the head of the
/// last verify, in step 3 and step 9, and asks for the part `## New
/// findings`. The start text of a first verify has none of it: with the
/// three parts out, the two texts are the same.
#[test]
fn the_start_text_of_a_fix_head_names_the_rule_and_the_last_head_and_a_first_one_does_not() {
    for checks in [&["CI".to_owned()][..], &[]] {
        let fix = wave_verify_start("o/r", "main", 12, PULL, "1a2b3c4d", checks, Some(LAST));
        let first = wave_verify_start("o/r", "main", 12, PULL, "1a2b3c4d", checks, None);

        let step3 = between(&fix, "\n3. ", "\n4. ");
        assert!(step3.contains(&format!("git diff {LAST} HEAD")), "{step3}");
        assert!(
            step3.contains("the last verify of the pull request failed at the commit"),
            "{step3}"
        );
        let step9 = between(&fix, "\n9. ", "\n10. ");
        for words in [
            "fails the verify only when it is one of these: a fail of a criterion of \"Done \
             when\"; a security fault; a finding of the last verify that stays; a fault in the \
             diff since 9f8e7d6c5b4a.",
            "Put each other finding that you show in the part `## New findings` of your result",
            "riff files the first 10 lines, each as an issue of its own in the Backlog, with the \
             line `Found by: #40`, and names the issue in the result.",
        ] {
            assert!(step9.contains(words), "no {words:?} in {step9}");
        }
        let step11 = between(&fix, "\n11. ", "\n\n");
        assert!(
            step11.contains("Then the part `## New findings` of step 9"),
            "{step11}"
        );

        for words in [
            LAST,
            "This head is a fix",
            "New findings",
            "Found by",
            "Backlog",
        ] {
            assert!(
                !first.contains(words),
                "{words:?} in the first verify: {first}"
            );
        }
        assert!(
            first.contains(&format!(
                "\n9. Check each finding in the code. {VERIFY_FIRST_RULE} {VERIFY_NOT_SHOWN}\n"
            )),
            "{first}"
        );
        let back = fix
            .replace(&format!("\n   {}", verify_fix_diff(LAST)), "")
            .replace(&verify_fix_rule(PULL, LAST), VERIFY_FIRST_RULE)
            .replace(
                "Then the part `## New findings` of step 9, when it has a line.\n   ",
                "",
            );
        assert_eq!(back, first);
    }
}

/// 01M7GDQWGCBVJE6S4MJDEJ6P8T: the diff of the start text of a fix head
/// runs as it is written, in the clone of a verify: at the head of the
/// pull request, detached. It shows only the fix. After a rebase, the
/// clone has no old head: the fetch of the start text gets it, as GitHub
/// gives each commit by its hash, and the diff runs.
#[test]
fn the_diff_since_the_last_head_runs_in_the_clone_of_a_verify_also_after_a_rebase() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    git(root, &["init", "-q", "--bare", "origin.git"]);
    let origin = root.join("origin.git");
    git(
        &origin,
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    );
    let url = format!("file://{}", origin.display());
    git(root, &["clone", "-q", &url, "work"]);
    let work = root.join("work");
    commit(&work, "base", "base\n");
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    git(&work, &["checkout", "-q", "-b", "fix"]);
    let pull = format!("HEAD:refs/pull/{PULL}/head");
    let last = commit(&work, "work", "the work\n");
    git(&work, &["push", "-q", "origin", &pull]);
    let head = commit(&work, "fix", "the fix\n");
    git(&work, &["push", "-q", "origin", &pull]);

    // The clone of a verify of the fix head.
    let verify = |name: &str, head: &str| {
        git(root, &["clone", "-q", &url, name]);
        let clone = root.join(name);
        git(
            &clone,
            &["fetch", "-q", "origin", &format!("refs/pull/{PULL}/head")],
        );
        git(&clone, &["checkout", "-q", "--detach", head]);
        clone
    };
    let clone = verify("verify", &head);
    let text = wave_verify_start("o/r", "main", 12, PULL, &head, &[], Some(&last));
    let diff = between(&text, "Read the diff since that commit: ", ". When git");
    assert_eq!(diff, format!("git diff {last} HEAD"));
    let changed = run(&clone, &format!("{diff} --name-only"));
    assert_eq!(changed, "fix", "the diff shows only the fix");

    // A rebase on a new main: the old head is on no branch of origin.
    git(&work, &["checkout", "-q", "main"]);
    commit(&work, "other", "other\n");
    git(&work, &["push", "-q", "origin", "HEAD:main"]);
    git(&work, &["checkout", "-q", "fix"]);
    git(&work, &["rebase", "-q", "main"]);
    let rebased = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["push", "-q", "-f", "origin", &pull]);
    let clone = verify("rebased", &rebased);
    assert!(!git_ok(
        &clone,
        &["cat-file", "-e", &format!("{last}^{{commit}}")]
    ));
    let text = wave_verify_start("o/r", "main", 12, PULL, &rebased, &[], Some(&last));
    let fetch = between(
        &text,
        &format!("When git has no commit {last}, run "),
        " first.",
    );
    assert_eq!(fetch, format!("git fetch origin {last}"));
    run(&clone, fetch);
    let diff = between(&text, "Read the diff since that commit: ", ". When git");
    let changed = run(&clone, &format!("{diff} --name-only"));
    let changed: Vec<&str> = changed.lines().collect();
    assert_eq!(changed, ["fix", "other"], "the fix and the new main");
}

/// "What fails the verify of a fix head" in the book says the rule, where
/// the daemon finds the last head, the part `## New findings`, and how to
/// list the issues of the new findings, with a diagram.
#[test]
fn the_book_says_what_fails_the_verify_of_a_fix_head() {
    let page = crate::book::page("how-it-works.md");
    let part = between(
        &page,
        "\n#### What fails the verify of a fix head\n",
        "\n#### ",
    );
    let flat = part.split_whitespace().collect::<Vec<_>>().join(" ");
    for words in [
        "A fix head is a new head of a pull request after a FAIL of its verify. Its verify \
         fails only on:",
        "- a criterion of `Done when:` that fails; - a security fault; - a finding of the \
         last verify that stays; - a fault that the verifier shows in the diff since the head \
         of the last verify.",
        "The first verify of a pull request fails on each finding that the verifier shows.",
        "The riff daemon finds the head of the last verify in the newest comment `verify \
         result:` of the pull request.",
        "So the comment counts only when the status `riff/verify` of its commit is a failure \
         and links to that comment.",
        "`git diff 9f8e7d6 HEAD`",
        "The verifier puts each other finding that it shows in the part `## New findings` of \
         its result, one line each",
        "The report of the verify of a fix head files each line as an issue of its own in the \
         Backlog, with the line `Found by: #40`.",
        "riff files no line that names an issue already.",
        "It files only the first 10 lines of a result",
        "the report of a first verify files nothing.",
        "A pass that riff refuses files nothing.",
        "It writes the number of each issue into the file of `--file`, also when the report \
         fails after the filing.",
        "```mermaid",
        "##### List the issues that the verifies of a pull request filed",
    ] {
        assert!(flat.contains(words), "no {words:?} in {part}");
    }
    // The command keeps only the issues whose last line is the line of
    // riff ([`riff::text::found_body`]).
    assert_eq!(
        crate::book::commands_in(part),
        [
            "gh issue list --state all --search '\"Found by: #40\" in:body' --json \
             number,title,body \\",
            "--jq '.[] | select(.body | test(\"\\nFound by: #40\\\\s*$\")) | \
             \"#\\(.number) \\(.title)\"'",
        ]
    );
    let body = riff::text::found_body(40, "1a2b3c4d", "Found by: #41", true);
    assert!(body.ends_with("\nFound by: #40\n"), "{body}");
    assert_eq!(body.matches("\nFound by: ").count(), 1, "{body}");
    assert_eq!(riff::council::found::MOST, 10);
    // The line of the result in the book is a line of the part.
    let example = between(part, "```markdown\n", "\n```");
    let lines = riff::council::found::unfiled(example);
    assert_eq!(lines.len(), 1, "{example}");
    assert!(lines[0].starts_with("crates/riff/src/text.rs:120: .riff/council/docs.md"));
    // The step of the council links the part.
    let council = crate::book::part("how-it-works.md", "Verify finished work");
    assert!(
        council.contains(
            "The verify of a fix head fails on fewer\n   findings: see\n   [What fails the \
             verify of a fix head](#what-fails-the-verify-of-a-fix-head)."
        ),
        "{council}"
    );
}
