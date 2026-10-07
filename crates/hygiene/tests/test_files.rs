//! Each file of `tests` of a crate is in a test binary
//! (01M4A4T6C5DH311AXMM6AG54DV). A crate with `autotests = false` builds
//! only the test binaries that its `Cargo.toml` names. So a new file that
//! is not a module of `tests/all.rs` would never run.
//!
//! No test of a shared test binary changes the environment of its
//! process (01M4BPJBTJ221A960NSVMPW8XM). Its tests run in parallel, and
//! a C call that reads the environment in another thread is not under
//! the lock of std.

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

/// The calls that change the environment of the process. The names
/// have no `(` here, so this file does not match itself.
const ENV_CHANGES: [&str; 2] = ["set_var", "remove_var"];

/// Each `.rs` file under `dir`, with its path from `dir`.
fn rust_files(dir: &Path, from: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, from, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path.strip_prefix(from).unwrap().to_owned());
        }
    }
}

/// The lines of `tests` of `krate` that change the environment, as
/// `FILE:LINE`. A file that is the path of a `[[test]]` of its own, not
/// `tests/all.rs`, is a binary of its own, and it is left out.
fn env_changes(krate: &Path) -> Vec<String> {
    let manifest = fs::read_to_string(krate.join("Cargo.toml")).unwrap_or_default();
    let alone: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("path = \"tests/")?.strip_suffix('"'))
        .filter(|path| *path != "all.rs")
        .collect();
    let tests = krate.join("tests");
    let mut files = Vec::new();
    rust_files(&tests, &tests, &mut files);
    files.sort();
    let calls = ENV_CHANGES.map(|name| format!("{name}("));
    let mut out = Vec::new();
    for file in files {
        let name = file.to_string_lossy().into_owned();
        if alone.contains(&name.as_str()) {
            continue;
        }
        let text = fs::read_to_string(tests.join(&file)).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if !code.starts_with("//") && calls.iter().any(|c| code.contains(c.as_str())) {
                out.push(format!("{name}:{}", n + 1));
            }
        }
    }
    out
}

#[test]
fn no_test_of_a_shared_binary_changes_the_environment() {
    let mut found = Vec::new();
    for entry in fs::read_dir(crates()).unwrap().flatten() {
        for line in env_changes(&entry.path()) {
            found.push(format!("{}/tests/{line}", entry.file_name().display()));
        }
    }
    assert!(
        found.is_empty(),
        "give the variable to the child (Command::env or env_remove), \
         or give the value as a parameter: {found:?}"
    );
}

#[test]
fn a_change_of_the_environment_is_found_outside_a_binary_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("tests/common")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        concat!(
            "[package]\nautotests = false\n\n",
            "[[test]]\nname = \"all\"\npath = \"tests/all.rs\"\n\n",
            "[[test]]\nname = \"alone\"\npath = \"tests/alone.rs\"\n",
        ),
    )
    .unwrap();
    let [set, remove] = ENV_CHANGES;
    let call = |name: &str| format!("fn t() {{\n    unsafe {{ std::env::{name}(\"X\") }};\n}}\n");
    let files = [
        ("all.rs", "mod a;\nmod common;\n".to_owned()),
        ("a.rs", call(remove)),
        ("common/mod.rs", format!("// a word: {set}(\n{}", call(set))),
        ("alone.rs", call(set)),
        ("clean.rs", "fn t() {}\n".to_owned()),
    ];
    for (file, text) in files {
        fs::write(dir.path().join("tests").join(file), text).unwrap();
    }
    assert_eq!(env_changes(dir.path()), ["a.rs:2", "common/mod.rs:3"]);
}
