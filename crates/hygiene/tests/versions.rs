//! `hygiene versions ROOT` on small trees, as `just versions` runs it
//! (01M5R2XVGMTB2HC9PRMS85ZWTY).

use std::path::Path;
use std::process::{Command, Output};

/// The text that the check must refuse. It is built, so that this file
/// names no version.
fn riff(version: &str) -> String {
    format!("riff {version}")
}

fn linux_only() -> String {
    format!("{} runs on Linux only.", riff("3.0"))
}

/// A tree with the file `path` and the allow list `allow`.
fn tree(path: &str, text: &str, allow: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in [(path, text), ("crates/hygiene/versions.allow", allow)] {
        let file = root.path().join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    root
}

fn versions(root: &Path) -> (Output, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hygiene"))
        .args(["versions", root.to_str().unwrap()])
        .output()
        .expect("run hygiene");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out, stderr)
}

#[test]
fn a_text_that_names_a_version_fails() {
    let text = format!("pub const TEXT: &str = \"{}\";\n", linux_only());
    let root = tree("crates/riff/src/text.rs", &text, "");
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("error: versions: crates/riff/src/text.rs:1: the line names"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("\"{}\"", riff("3.0"))), "{stderr}");
    assert!(stderr.contains("rule(s) of the versions check"), "{stderr}");
}

#[test]
fn a_page_of_the_book_that_names_a_version_fails() {
    let root = tree(
        "docs/src/start-a-riff.md",
        &format!("{}\n", linux_only()),
        "",
    );
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("docs/src/start-a-riff.md:1:"), "{stderr}");
}

#[test]
fn a_line_of_the_allow_list_lets_the_text_pass() {
    let text = format!("The file of {}: remove it.\n", riff("3.3.0"));
    let allow = format!(
        "# A comment.\ncrates/riff/src/old.rs | file of riff {} | the file that an old release left\n",
        "3.3.0"
    );
    let root = tree("crates/riff/src/old.rs", &text, &allow);
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok: the version names in"));
}

#[test]
fn the_allow_list_does_not_allow_another_file() {
    let allow = format!(
        "crates/riff/src/old.rs | riff {} | the file that an old release left\n",
        "3.3.0"
    );
    let text = format!("riff {} left it.\n", "3.3.0");
    let root = tree("crates/riff/src/new.rs", &text, &allow);
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("crates/riff/src/new.rs:1:"), "{stderr}");
    assert!(stderr.contains("versions.allow:1: no line of"), "{stderr}");
}

#[test]
fn a_line_of_the_allow_list_that_allows_nothing_fails() {
    let root = tree(
        "crates/riff/src/text.rs",
        "A text with no version.\n",
        "crates/riff/src/text.rs | a name | an old reason\n",
    );
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("versions.allow:1: no line of crates/riff/src/text.rs"),
        "{stderr}"
    );
}

#[test]
fn the_history_of_the_requirements_may_name_a_version() {
    let root = tree(
        "docs/src/requirements-history.md",
        &format!("Superseded by {}.\n", riff("4.0")),
        "",
    );
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(0), "{stderr}");
}

#[test]
fn the_requirements_of_today_may_not_name_a_version() {
    let root = tree(
        "docs/src/requirements.md",
        &format!("- **R1** {} runs on Linux only.\n", riff("3.0")),
        "",
    );
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("docs/src/requirements.md:1:"), "{stderr}");
}

#[test]
fn a_root_with_no_allow_list_is_a_tool_error() {
    let root = tempfile::tempdir().unwrap();
    let (out, stderr) = versions(root.path());
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("versions.allow"), "{stderr}");
}

/// The tree of this repository passes, with its allow list.
#[test]
fn this_repository_names_no_version_but_the_allowed() {
    let top = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (out, stderr) = versions(&top);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
}
