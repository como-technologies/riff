//! Each file of `tests` of a crate is in a test binary
//! (01M4A4T6C5DH311AXMM6AG54DV). A crate with `autotests = false` builds
//! only the test binaries that its `Cargo.toml` names. So a new file that
//! is not a module of `tests/all.rs` would never run.

use std::fs;
use std::path::{Path, PathBuf};

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The files of `tests` of `krate` that no test binary holds: not a
/// module of `tests/all.rs`, and not the path of a `[[test]]`.
fn left_out(krate: &Path) -> Vec<String> {
    let manifest = fs::read_to_string(krate.join("Cargo.toml")).unwrap();
    if !manifest.contains("autotests = false") {
        return Vec::new();
    }
    let all = fs::read_to_string(krate.join("tests/all.rs")).unwrap_or_default();
    let mut held: Vec<String> = Vec::new();
    let mut path = None;
    for line in all.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#[path = \"") {
            path = rest.strip_suffix("\"]").map(str::to_owned);
        } else if let Some(name) = line.strip_prefix("mod ").and_then(|l| l.strip_suffix(';')) {
            held.push(path.take().unwrap_or(format!("{name}.rs")));
        }
    }
    for line in manifest.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("path = \"tests/") {
            held.extend(rest.strip_suffix('"').map(str::to_owned));
        }
    }
    let mut out: Vec<String> = fs::read_dir(krate.join("tests"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".rs") && !held.contains(name))
        .collect();
    out.sort();
    out
}

#[test]
fn each_test_file_is_in_a_test_binary() {
    let mut missing = Vec::new();
    for entry in fs::read_dir(crates()).unwrap().flatten() {
        let krate = entry.path();
        for file in left_out(&krate) {
            missing.push(format!("{}/tests/{file}", entry.file_name().display()));
        }
    }
    assert!(
        missing.is_empty(),
        "add each file as `mod NAME;` to tests/all.rs of its crate: {missing:?}"
    );
}

#[test]
fn a_file_with_no_module_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("tests")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        concat!(
            "[package]\nautotests = false\n\n",
            "[[test]]\nname = \"all\"\npath = \"tests/all.rs\"\n\n",
            "[[test]]\nname = \"alone\"\npath = \"tests/alone.rs\"\n",
        ),
    )
    .unwrap();
    fs::write(
        dir.path().join("tests/all.rs"),
        "mod common;\n\nmod a;\n#[path = \"loop.rs\"]\nmod loops;\n",
    )
    .unwrap();
    for file in ["a.rs", "loop.rs", "alone.rs", "new.rs"] {
        fs::write(dir.path().join("tests").join(file), "").unwrap();
    }
    assert_eq!(left_out(dir.path()), ["new.rs"]);
}
