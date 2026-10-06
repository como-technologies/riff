//! `hygiene ci` in a git repository and `hygiene wrap` in a directory,
//! as `just ci` and `just wrap` run them (01M3WNMKB6PAP6J0QXX4A684HH,
//! 01M3WNN7VQJKN5MJH7JN50VF4D, 01M3WNN836EFG7GJQKZRTSK5FT). The
//! recipes of the justfile and the Gate use the names that `hygiene ci`
//! prints.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", text(&out.stderr));
    text(&out.stdout)
}

fn write(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A git repository on the branch `work`. Its one commit is also
/// `origin/main`, as after a fetch.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path();
    git(top, &["init", "-q", "-b", "work"]);
    write(top, ".gitignore", "target/\n");
    write(top, "crates/a/src/lib.rs", "//! A.\n");
    write(top, "docs/src/how-it-works.md", "# How\n\nText.\n");
    git(top, &["add", "-A"]);
    git(top, &["commit", "-q", "-m", "first"]);
    git(top, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    dir
}

fn commit(top: &Path, message: &str) {
    git(top, &["add", "-A"]);
    git(top, &["commit", "-q", "-m", message]);
}

fn hygiene(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// The recipe that `hygiene ci` prints in `dir`, and its line for the
/// person.
fn ci(dir: &Path) -> (String, String) {
    let out = hygiene(dir, &["ci"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    (
        text(&out.stdout).trim().to_owned(),
        text(&out.stderr).trim().to_owned(),
    )
}

#[test]
fn a_branch_with_only_a_review_file_gets_the_text_checks() {
    let dir = repo();
    write(dir.path(), "design/reviews/x.md", "# Review\n");
    commit(dir.path(), "a review");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-text");
    assert_eq!(
        line,
        "just ci runs only the text checks (book, reqs, wrap): design/reviews/x.md \
         is the only file that differs from origin/main, and it is text"
    );
}

#[test]
fn a_branch_with_only_a_book_page_gets_the_text_checks() {
    let dir = repo();
    write(dir.path(), "docs/src/how-it-works.md", "# How\n\nNew.\n");
    commit(dir.path(), "the book");
    assert_eq!(ci(dir.path()).0, "ci-text");
}

#[test]
fn a_branch_with_a_rust_file_and_a_markdown_file_gets_each_check() {
    let dir = repo();
    write(dir.path(), "crates/a/src/lib.rs", "//! B.\n");
    write(dir.path(), "docs/src/how-it-works.md", "# How\n\nNew.\n");
    commit(dir.path(), "code and text");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full");
    assert_eq!(
        line,
        "just ci runs each check: crates/a/src/lib.rs is not a text file"
    );
}

#[test]
fn a_change_that_is_not_committed_counts() {
    let dir = repo();
    write(dir.path(), "design/reviews/x.md", "# Review\n");
    commit(dir.path(), "a review");
    write(dir.path(), "crates/a/src/lib.rs", "//! B.\n");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full", "{line}");
    git(dir.path(), &["checkout", "-q", "crates/a/src/lib.rs"]);
    assert_eq!(ci(dir.path()).0, "ci-text");
    write(dir.path(), "crates/a/src/new.rs", "//! New.\n");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full");
    assert!(
        line.ends_with("crates/a/src/new.rs is not a text file"),
        "{line}"
    );
}

#[test]
fn a_rust_file_that_becomes_a_markdown_file_gets_each_check() {
    let dir = repo();
    git(dir.path(), &["mv", "crates/a/src/lib.rs", "notes.md"]);
    commit(dir.path(), "a rename");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full");
    assert!(
        line.ends_with("crates/a/src/lib.rs is not a text file"),
        "{line}"
    );
}

#[test]
fn a_change_in_a_directory_below_the_top_counts() {
    let dir = repo();
    write(dir.path(), "crates/a/src/lib.rs", "//! B.\n");
    assert_eq!(ci(&dir.path().join("docs")).0, "ci-full");
}

#[test]
fn no_diff_and_no_base_get_each_check() {
    let dir = repo();
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full");
    assert_eq!(
        line,
        "just ci runs each check: no file differs from origin/main"
    );

    git(
        dir.path(),
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    write(dir.path(), "design/reviews/x.md", "# Review\n");
    let (recipe, line) = ci(dir.path());
    assert_eq!(recipe, "ci-full");
    assert_eq!(
        line,
        "just ci runs each check: git cannot compare this tree with origin/main"
    );

    let out = hygiene(dir.path(), &["ci", "work"]);
    assert_eq!(text(&out.stdout).trim(), "ci-text");

    let plain = tempfile::tempdir().unwrap();
    assert_eq!(ci(plain.path()).0, "ci-full");
}

/// A git repository whose workspace has the crates `a` and `b`, where
/// `b` depends on `a`. Its one commit is also `origin/main`.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path();
    git(top, &["init", "-q", "-b", "work"]);
    write(top, ".gitignore", "target/\n");
    write(
        top,
        "Cargo.toml",
        "[workspace]\nresolver = \"3\"\nmembers = [\"crates/*\"]\n",
    );
    let package = |name: &str, deps: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{deps}")
    };
    write(top, "crates/a/Cargo.toml", &package("a", ""));
    write(top, "crates/a/src/lib.rs", "//! A.\n");
    write(
        top,
        "crates/b/Cargo.toml",
        &package("b", "a = { path = \"../a\" }\n"),
    );
    write(top, "crates/b/src/lib.rs", "//! B.\n");
    write(top, "docs/src/how-it-works.md", "# How\n\nText.\n");
    commit(top, "first");
    git(top, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    dir
}

/// The arguments of `cargo test` that `hygiene crates` prints in `dir`,
/// and its line for the person.
fn crates(dir: &Path) -> (String, String) {
    let out = hygiene(dir, &["crates"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    (
        text(&out.stdout).trim().to_owned(),
        text(&out.stderr).trim().to_owned(),
    )
}

/// `just check` tests the crates of the diff and their dependents
/// (01M49HAZA5K08XW2JQ11TG87JP).
#[test]
fn crates_names_the_changed_crates_and_their_dependents() {
    let dir = workspace();
    write(dir.path(), "crates/a/src/lib.rs", "//! A, new.\n");
    let (args, line) = crates(dir.path());
    assert_eq!(args, "-p a -p b");
    assert_eq!(
        line,
        "just check runs the tests of a, b: a differs from origin/main, and each crate \
         that depends on a changed crate runs too"
    );

    let dir = workspace();
    write(dir.path(), "crates/b/src/lib.rs", "//! B, new.\n");
    commit(dir.path(), "b");
    assert_eq!(crates(dir.path()).0, "-p b");

    let dir = workspace();
    write(dir.path(), "docs/src/how-it-works.md", "# How\n\nNew.\n");
    let (args, line) = crates(dir.path());
    assert_eq!(args, "");
    assert_eq!(
        line,
        "just check runs no test: no crate differs from origin/main"
    );

    let dir = workspace();
    write(dir.path(), "justfile", "default:\n");
    let (args, line) = crates(dir.path());
    assert_eq!(args, "--workspace");
    assert_eq!(
        line,
        "just check runs the tests of each crate: justfile is in no crate, and it is not text"
    );
}

#[test]
fn wrap_fails_on_a_long_line_of_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let long = "word ".repeat(15);
    write(dir.path(), "src/a.md", "# A\n\nShort.\n");
    write(dir.path(), "src/notes.txt", &long);
    let out = hygiene(dir.path(), &["wrap", "src"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "ok: the wrap of the pages in src\n");

    write(dir.path(), "src/b.md", &format!("# B\n\n{long}\n"));
    let out = hygiene(dir.path(), &["wrap", "src"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("error: wrap: src/b.md:3: the line has 75 characters"),
        "{stderr}"
    );
    assert!(
        stderr.contains("breaks 1 rule(s) of the wrap check"),
        "{stderr}"
    );

    let out = hygiene(dir.path(), &["wrap", "gone"]);
    assert_eq!(out.status.code(), Some(2));
}

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The names after the colon of the recipe `name` of the justfile.
fn recipe(name: &str) -> Vec<String> {
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    let start = format!("{name}:");
    let line = justfile
        .lines()
        .find(|line| line.starts_with(&start))
        .unwrap_or_else(|| panic!("the justfile has no recipe {name}"));
    line[start.len()..]
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_text_recipe_of_the_justfile_runs_no_code_check() {
    assert_eq!(recipe("ci-text"), ["book", "reqs", "wrap"]);
    assert_eq!(
        recipe("ci-checks"),
        ["fmt-check", "lint", "test", "doc", "book", "reqs", "wrap"]
    );
    assert!(recipe("ci").is_empty(), "ci has no fixed checks");
    assert!(recipe("ci-full").is_empty(), "ci-full takes the lock first");
    let justfile = std::fs::read_to_string(top().join("justfile")).unwrap();
    let full = &justfile[justfile.find("\nci-full:").unwrap()..];
    let full = &full[..full.find("\n\n").unwrap()];
    assert!(full.contains("ci_lock \"$PWD/target\""), "{full}");
    assert!(full.ends_with("{{just_executable()}} ci-checks"), "{full}");
    assert!(justfile.contains("recipe=$(cargo run -q -p hygiene -- ci)"));
    assert!(justfile.contains("cargo run -q -p hygiene -- wrap docs/src"));
}

#[test]
fn the_gate_on_github_runs_each_check() {
    let workflow = std::fs::read_to_string(top().join(".github/workflows/ci.yml")).unwrap();
    assert!(
        workflow.contains("run: just ci-full\n"),
        "the Gate runs ci-full"
    );
    assert!(
        !workflow.contains("run: just ci\n"),
        "the Gate does not run ci"
    );
}
