//! The tables of shared surfaces in the rustdoc of `riff::confine` and
//! in `how-it-works.md` have the same rows, and each test that a row
//! names is in a source file (01M4DDZ8DFY1SP1AVVRSD0DRV4).

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The text of each `.rs` file under `dir`.
fn sources(dir: &Path, text: &mut String) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "target") {
            sources(&path, text);
        } else if path.extension().is_some_and(|e| e == "rs") {
            *text += &fs::read_to_string(&path).unwrap_or_default();
        }
    }
}

#[test]
fn each_shared_surface_is_in_the_rustdoc_and_the_book_with_a_real_test() {
    let root = root();
    let confine = fs::read_to_string(root.join("crates/riff/src/confine.rs")).unwrap();
    let book = fs::read_to_string(root.join("docs/src/how-it-works.md")).unwrap();
    let mut text = String::new();
    sources(&root.join("crates"), &mut text);
    let tests = hygiene::surfaces::test_names(&text);
    let errors = hygiene::surfaces::check(&hygiene::surfaces::doc_of(&confine), &book, &tests);
    let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
