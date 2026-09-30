//! `hygiene book` on small books, built with the real `mdbook`, as
//! `just book` runs it. `just init` installs `mdbook`.

use std::path::Path;
use std::process::{Command, Output};

/// A book in a new directory: `SUMMARY.md`, the page `a.md` with `page`,
/// and `code.rs` with the anchor `good`.
fn book(page: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        dir.path().join("book.toml"),
        "[book]\ntitle = \"t\"\nsrc = \"src\"\n",
    )
    .unwrap();
    std::fs::write(src.join("SUMMARY.md"), "# Summary\n\n- [A](a.md)\n").unwrap();
    std::fs::write(
        src.join("code.rs"),
        "// ANCHOR: good\nfn good() {}\n// ANCHOR_END: good\n",
    )
    .unwrap();
    std::fs::write(src.join("a.md"), page).unwrap();
    dir
}

fn check(dir: &Path) -> (Output, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(["book", dir.to_str().unwrap()])
        .output()
        .expect("run hygiene");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_ne!(
        out.status.code(),
        Some(2),
        "hygiene book could not run mdbook; install it with just init:\n{stderr}"
    );
    (out, stderr)
}

#[test]
fn a_good_book_with_an_escaped_example_passes() {
    let dir = book(
        "# A\n\n```rust\n{{#include code.rs:good}}\n```\n\n\
         An example:\n\n```text\n\\{{#include code.rs:good}}\n```\n",
    );
    let (out, stderr) = check(dir.path());
    assert!(out.status.success(), "{stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok: the book in"));
}

#[test]
fn a_missing_anchor_fails() {
    let dir = book("# A\n\n```rust\n{{#include code.rs:gone}}\n```\n");
    let (out, stderr) = check(dir.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("error: empty-code: "), "{stderr}");
    assert!(stderr.contains("a.html:"), "{stderr}");
    assert!(stderr.contains("rule(s) of the book check"), "{stderr}");
}

#[test]
fn a_missing_file_in_a_code_block_fails() {
    let dir = book("# A\n\n```rust\n{{#include gone.rs}}\n```\n");
    let (out, stderr) = check(dir.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("error: mdbook: ERROR"), "{stderr}");
    assert!(!stderr.contains("error: include: "), "{stderr}");
}

#[test]
fn a_missing_file_in_the_text_fails() {
    let dir = book("# A\n\nText {{#include gone.rs}} here.\n");
    let (out, stderr) = check(dir.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("error: mdbook: ERROR"), "{stderr}");
    assert!(stderr.contains("error: include: "), "{stderr}");
    assert!(stderr.contains("{{#include gone.rs}}"), "{stderr}");
}

#[test]
fn a_directory_with_no_book_is_a_tool_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(["book", dir.path().join("none").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
